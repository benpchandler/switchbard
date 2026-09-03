//! Column, filter, and sort pickers, and the one key handler every picker shares.

use std::str::FromStr;

use crossterm::event::{KeyCode, KeyEvent};

use crate::app::{App, Mode};
use crate::columns::Column;
use crate::picker::{ColumnPurpose, PaintPick, Payload, PickOption, PickerPurpose, ValuePicker};
use crate::sort::{self, Sort};
use crate::tasks::{self, Filter, FilterField};

impl App {
    /// After `f`/`s`: shown columns first, numbered as in the header, then hidden ones.
    pub(super) fn open_column_chooser(&mut self, purpose: ColumnPurpose) {
        self.column_purpose = purpose;
        let options = self.column_picker_options();
        self.open_picker(PickerPurpose::ChooseColumn(purpose), options);
        self.status.clear();
    }

    pub(super) fn open_column_purpose(&mut self, column: Column) {
        match self.column_purpose {
            ColumnPurpose::Filter => self.open_filter_picker(column),
            ColumnPurpose::Sort => self.open_sort_picker(column),
        }
    }

    pub(super) fn open_picker(&mut self, purpose: PickerPurpose, options: Vec<PickOption>) {
        self.picker = Some(ValuePicker::new(purpose, options));
        self.mode = Mode::PickValue;
    }

    pub(super) fn open_columns_picker(&mut self) {
        let options = self.column_picker_options();
        self.open_picker(PickerPurpose::Columns, options);
        self.telemetry.record("action", "columns");
    }

    /// Shown columns first in display order, then the hidden ones.
    pub(super) fn column_picker_options(&self) -> Vec<PickOption> {
        self.state
            .columns
            .iter()
            .map(|column| PickOption::column(*column, false))
            .chain(
                Column::ALL
                    .iter()
                    .filter(|column| !self.state.columns.contains(column))
                    .map(|column| PickOption::column(*column, true)),
            )
            .collect()
    }

    pub(super) fn toggle_column(&mut self, column: Column) {
        match self.state.columns.iter().position(|shown| *shown == column) {
            Some(index) if self.state.columns.len() > 1 => {
                self.state.columns.remove(index);
            }
            Some(_) => self.status = "at least one column must stay".to_string(),
            None => self.state.columns.push(column),
        }
        self.telemetry
            .record("action", format!("column_toggle {}", column.name()));
    }

    pub(super) fn shown_column_options(&self) -> Vec<PickOption> {
        self.state
            .columns
            .iter()
            .map(|column| PickOption::column(*column, false))
            .collect()
    }

    /// `c m 3 1 2`: the columns numbered by `placed` (as the header showed them
    /// when `m` was pressed) come first in that order; the rest keep theirs.
    pub(super) fn reorder_columns(&mut self, placed: &[usize]) {
        let original = self
            .move_origin
            .clone()
            .unwrap_or_else(|| self.state.columns.clone());
        let mut next: Vec<Column> = placed
            .iter()
            .filter_map(|&n| original.get(n - 1).copied())
            .collect();
        let rest: Vec<Column> = original
            .iter()
            .copied()
            .filter(|c| !next.contains(c))
            .collect();
        next.extend(rest);
        self.state.columns = next;
        self.telemetry.record("action", "column_move");
    }

    /// The glyphs a column's values take, in vocabulary order, for its header.
    pub fn glyph_legend(&self, column: Column) -> String {
        let Some(field) = column.filter_field() else {
            return String::new();
        };
        let mut values: Vec<String> = tasks::field_values(&self.tasks, field)
            .into_iter()
            .map(|(value, _)| value)
            .collect();
        values.sort_by_key(|value| (column.vocabulary_rank(value), value.clone()));
        values
            .iter()
            .map(|value| self.config.glyph(column, value))
            .collect()
    }

    /// `g` in the columns picker: glyphs instead of text, for categorical columns.
    pub(super) fn toggle_glyph_column(&mut self, column: Column) {
        if column.filter_field().is_none() || column == Column::Id {
            self.status = format!("{} has no glyphs: it is free text", column.name());
            return;
        }
        match self.state.glyph_columns.iter().position(|c| *c == column) {
            Some(index) => {
                self.state.glyph_columns.remove(index);
                self.status = format!("{} shows text", column.name());
            }
            None => {
                self.state.glyph_columns.push(column);
                self.status = format!("{} shows glyphs", column.name());
            }
        }
        self.telemetry
            .record("action", format!("glyph_toggle {}", column.name()));
    }

    pub(super) fn move_column(&mut self, column: Column, delta: isize) {
        let Some(index) = self.state.columns.iter().position(|shown| *shown == column) else {
            return;
        };
        let target = index as isize + delta;
        if target < 0 || target >= self.state.columns.len() as isize {
            return;
        }
        self.state.columns.swap(index, target as usize);
        self.telemetry
            .record("action", format!("column_move {} {delta}", column.name()));
    }

    pub(super) fn open_filter_picker(&mut self, column: Column) {
        match column.filter_field() {
            Some(field) => {
                let values = tasks::field_values(&self.tasks, field);
                if values.is_empty() {
                    self.status = format!("no {} values to pick from", field.keyword());
                    return;
                }
                let options = values
                    .into_iter()
                    .map(|(value, count)| PickOption::text(value, count))
                    .collect();
                self.open_picker(PickerPurpose::Filter(field), options);
            }
            None => {
                self.mode = Mode::Filter;
                self.status = format!("{} is free text: type to search", column.header());
            }
        }
        self.telemetry
            .record("action", format!("filter_column {}", column.header()));
    }

    pub(super) fn open_sort_picker(&mut self, column: Column) {
        let mut options: Vec<PickOption> = sort::orders_for(column)
            .into_iter()
            .map(|order| PickOption::numbered(order.label(column), Payload::Order(order)))
            .collect();
        options.push(PickOption::numbered("none", Payload::NoSort));
        self.open_picker(PickerPurpose::Sort(column), options);
        self.telemetry
            .record("action", format!("sort_column {}", column.header()));
    }

    pub(super) fn handle_pick_value_key(&mut self, event: KeyEvent) {
        let Some(picker) = self.picker.as_mut() else {
            self.mode = Mode::Browse;
            return;
        };
        let last = picker.matching().len().saturating_sub(1);
        let purpose = picker.purpose.clone();
        let typed_empty = picker.typed.is_empty();
        match event.code {
            KeyCode::Esc => {
                self.picker = None;
                self.mode = Mode::Browse;
                self.paint_return = None;
                self.move_origin = None;
            }
            KeyCode::Down => picker.selected = (picker.selected + 1).min(last),
            KeyCode::Up => picker.selected = picker.selected.saturating_sub(1),
            KeyCode::Char('j') if typed_empty => picker.selected = (picker.selected + 1).min(last),
            KeyCode::Char('k') if typed_empty => {
                picker.selected = picker.selected.saturating_sub(1)
            }
            KeyCode::Char(digit)
                if digit.is_ascii_digit() && matches!(purpose, PickerPurpose::MoveColumns(_)) =>
            {
                let PickerPurpose::MoveColumns(mut placed) = purpose else {
                    return;
                };
                let index = digit.to_digit(10).unwrap_or(0) as usize;
                if index == 0 || index > self.state.columns.len() || placed.contains(&index) {
                    return;
                }
                placed.push(index);
                self.reorder_columns(&placed);
                let options = self.shown_column_options();
                let done = placed.len();
                self.open_picker(PickerPurpose::MoveColumns(placed), options);
                if let Some(picker) = self.picker.as_mut() {
                    picker.selected = done.saturating_sub(1);
                }
            }
            KeyCode::Char(digit) if digit.is_ascii_digit() && typed_empty => {
                picker.number.push(digit);
                let index: usize = picker.number.parse().unwrap_or(0);
                let count = picker.numbered_count();
                let could_extend = index * 10 <= count;
                if index == 0 || index > count {
                    picker.number.clear();
                } else if could_extend {
                    // Wait: a second digit may still follow (1 when there are 10+).
                } else {
                    picker.number.clear();
                    if let Some(position) = picker.position_of_number(index) {
                        picker.selected = position;
                        self.apply_picked_value();
                    }
                }
            }
            KeyCode::Enter if !picker.number.is_empty() => {
                let index: usize = picker.number.parse().unwrap_or(0);
                picker.number.clear();
                if let Some(position) = picker.position_of_number(index) {
                    picker.selected = position;
                    self.apply_picked_value();
                }
            }
            KeyCode::Enter if matches!(purpose, PickerPurpose::MoveColumns(_)) => {
                self.move_origin = None;
                let options = self.column_picker_options();
                self.open_picker(PickerPurpose::Columns, options);
            }
            KeyCode::Enter => self.apply_picked_value(),
            KeyCode::Delete | KeyCode::Backspace
                if purpose == PickerPurpose::PaintTarget && typed_empty =>
            {
                self.picker = None;
                self.mode = Mode::Browse;
                self.clear_all_paint();
            }
            KeyCode::Delete | KeyCode::Backspace
                if purpose == PickerPurpose::PaintRules && typed_empty =>
            {
                let index = picker.selected;
                if index < self.state.paint.len() {
                    self.state.paint.remove(index);
                    self.telemetry.record("action", "paint_rule_delete");
                }
                self.open_paint_rules_picker();
                if self.state.paint.is_empty() {
                    self.status = "no paint rules left".to_string();
                    self.picker = None;
                    self.mode = Mode::Browse;
                }
            }
            KeyCode::Char(direction @ ('J' | 'K')) if purpose == PickerPurpose::PaintRules => {
                let delta = if direction == 'J' { 1 } else { -1 };
                let selected = picker.selected;
                let moved_to = self.move_paint_rule(selected, delta);
                self.open_paint_rules_picker();
                if let Some(picker) = self.picker.as_mut() {
                    picker.selected = moved_to;
                }
            }
            KeyCode::Left | KeyCode::Char('h')
                if typed_empty
                    && matches!(
                        purpose,
                        PickerPurpose::PaintValues(_)
                            | PickerPurpose::PaintColor(_)
                            | PickerPurpose::PaintColumn
                            | PickerPurpose::PaintRules
                    ) =>
            {
                self.paint_back();
            }
            KeyCode::Char(' ') if matches!(purpose, PickerPurpose::PaintColor(_)) => {
                let PickerPurpose::PaintColor(pick) = purpose else {
                    return;
                };
                self.picker = None;
                self.mode = Mode::Browse;
                self.apply_paint(pick, "none");
            }
            KeyCode::Char(' ') => self.toggle_picked_value(),
            KeyCode::Char('m') if purpose == PickerPurpose::Columns && typed_empty => {
                self.move_origin = Some(self.state.columns.clone());
                let options = self.shown_column_options();
                self.open_picker(PickerPurpose::MoveColumns(Vec::new()), options);
            }
            KeyCode::Char('g') if purpose == PickerPurpose::Columns && typed_empty => {
                if let Some(Payload::Column(column)) = picker.highlighted().map(|o| o.payload) {
                    self.toggle_glyph_column(column);
                }
            }
            KeyCode::Char(direction @ ('J' | 'K')) if purpose == PickerPurpose::Columns => {
                if let Some(Payload::Column(column)) = picker.highlighted().map(|o| o.payload) {
                    let delta = if direction == 'J' { 1 } else { -1 };
                    let moved_to = picker.selected as isize + delta;
                    self.move_column(column, delta);
                    let options = self.column_picker_options();
                    if let Some(picker) = self.picker.as_mut() {
                        picker.options = options;
                        picker.selected =
                            moved_to.clamp(0, picker.options.len() as isize - 1) as usize;
                    }
                }
            }
            KeyCode::Char(letter) if typed_empty && picker.position_of_key(letter).is_some() => {
                picker.selected = picker.position_of_key(letter).unwrap_or(0);
                self.apply_picked_value();
            }
            KeyCode::Backspace => {
                picker.typed.pop();
                picker.selected = 0;
            }
            KeyCode::Char(c) => {
                picker.typed.push(c);
                picker.selected = 0;
                if picker.matching().len() == 1 {
                    self.apply_picked_value();
                }
            }
            _ => {}
        }
    }

    pub(super) fn toggle_picked_value(&mut self) {
        let Some(picker) = self.picker.as_ref() else {
            return;
        };
        let Some(option) = picker.highlighted() else {
            return;
        };
        match (&picker.purpose, option.payload) {
            (PickerPurpose::Filter(field), Payload::Text(value)) => {
                self.toggle_filter_value(*field, &value)
            }
            (PickerPurpose::Columns, Payload::Column(column)) => {
                self.toggle_column(column);
                let options = self.column_picker_options();
                if let Some(picker) = self.picker.as_mut() {
                    picker.options = options;
                }
            }
            _ => {}
        }
    }

    pub(super) fn toggle_filter_value(&mut self, field: FilterField, value: &str) {
        let Some(picker) = self.picker.as_ref() else {
            return;
        };
        let all: Vec<String> = picker
            .options
            .iter()
            .map(|option| option.label.clone())
            .collect();
        let mut shown: Vec<String> = all
            .iter()
            .filter(|candidate| Filter::field_allows(&self.state.filter, field, candidate))
            .cloned()
            .collect();
        match shown.iter().position(|candidate| candidate == value) {
            Some(index) => {
                shown.remove(index);
            }
            None => shown.push(value.to_string()),
        }
        let text = Filter::with_shown(&self.state.filter, field, &all, &shown);
        self.set_filter(text);
        self.telemetry.record(
            "action",
            format!("filter_toggle {}:{value}", field.keyword()),
        );
    }

    pub(super) fn apply_picked_value(&mut self) {
        let Some(picker) = self.picker.take() else {
            return;
        };
        self.mode = Mode::Browse;
        let typed = picker.typed.trim().to_string();
        let picked = picker.highlighted().or_else(|| match picker.purpose {
            PickerPurpose::PaintColor(_) if ratatui::style::Color::from_str(&typed).is_ok() => {
                Some(PickOption::text(typed.clone(), 0))
            }
            _ => None,
        });
        let Some(picked) = picked else {
            self.status = format!("nothing matches '{typed}'");
            return;
        };
        match (picker.purpose, picked.payload) {
            (PickerPurpose::Filter(field), Payload::Text(value)) => {
                let text = Filter::with_only(&self.state.filter, field, &value);
                self.set_filter(text);
                self.telemetry
                    .record("action", format!("filter_pick {}:{value}", field.keyword()));
            }
            (PickerPurpose::Sort(column), Payload::Order(order)) => {
                self.state.sort = Some(Sort { column, order });
                self.refilter();
                self.telemetry.record(
                    "action",
                    format!("sort_pick {}:{:?}", column.header(), order),
                );
            }
            (PickerPurpose::Sort(_), Payload::NoSort) => {
                self.state.sort = None;
                self.refilter();
                self.telemetry.record("action", "sort_pick none");
            }
            (PickerPurpose::ChooseColumn(_), Payload::Column(column)) => {
                self.open_column_purpose(column)
            }
            (PickerPurpose::Columns, Payload::Column(column)) => {
                let adding = !self.state.columns.contains(&column);
                self.toggle_column(column);
                if adding {
                    // Stay open with the new column highlighted so it can be placed.
                    let options = self.column_picker_options();
                    let highlight = self.state.columns.len().saturating_sub(1);
                    self.open_picker(PickerPurpose::Columns, options);
                    if let Some(picker) = self.picker.as_mut() {
                        picker.selected = highlight;
                    }
                    self.status = format!(
                        "{} added as column {} · m then numbers to reorder · esc",
                        column.name(),
                        self.state.columns.len()
                    );
                }
            }
            (PickerPurpose::PaintTarget, Payload::DeleteAllPaint) => self.clear_all_paint(),
            (PickerPurpose::PaintTarget, Payload::OrderRules) => self.open_paint_rules_picker(),
            (PickerPurpose::PaintTarget, Payload::WholeColumn) => self.open_paint_column_picker(),
            (PickerPurpose::PaintTarget, Payload::Column(column)) => {
                self.paint_column_entry(column)
            }
            (PickerPurpose::PaintTarget, Payload::ThisRow(id)) => {
                self.open_paint_color_picker(PaintPick::Rows(format!("id:{id}")))
            }
            (PickerPurpose::PaintTarget, Payload::FilteredRows(filter)) => {
                self.open_paint_color_picker(PaintPick::Rows(filter))
            }
            (PickerPurpose::PaintColumn, Payload::Column(column)) => {
                self.paint_return = None;
                self.open_paint_color_picker(PaintPick::Column(column));
            }
            (PickerPurpose::PaintValues(column), Payload::Auto) => {
                self.paint_auto(column);
                self.open_paint_values_picker(column);
            }
            (PickerPurpose::PaintValues(column), Payload::Text(value)) => {
                self.open_paint_color_picker(PaintPick::Value(column, value))
            }
            (PickerPurpose::PaintColor(pick), Payload::Text(color)) => {
                self.apply_paint(pick, &color)
            }
            (PickerPurpose::PaintColor(pick), Payload::NoColor) => self.apply_paint(pick, "none"),
            (PickerPurpose::PaintRules, Payload::Rule(index)) => {
                self.open_paint_rules_picker();
                if let Some(picker) = self.picker.as_mut() {
                    picker.selected = index;
                }
            }
            (purpose, payload) => {
                self.fail(format!("{purpose:?} cannot take {payload:?}"));
            }
        }
    }
}
