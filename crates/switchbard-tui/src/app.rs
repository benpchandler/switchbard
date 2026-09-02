//! Application state and the single place key events turn into state changes.

use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::time::{Instant, SystemTime};

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind};
use switchbard_core::BacklogTask;

use crate::ball::Ball;
use crate::config::{self, Action, Column, Config, KeyChord};
use crate::paint::{self, PaintRule, NAMED_COLORS};
use crate::picker::{ColumnPurpose, PaintPick, Payload, PickOption, PickerPurpose, ValuePicker};
use crate::report::{self, ReportContext, ReportKind};
use crate::sort::{self, Sort};
use crate::tasks::{self, Filter, FilterField};
use crate::telemetry::Telemetry;
use crate::views::{self, ViewState, ViewStore};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Browse,
    Filter,
    Command,
    PickValue,
    /// After `v`: a digit opens that slot, `s` starts a save.
    ViewChord,
    /// After `v s`: a digit or `d` (slot 1) picks the slot to save into.
    ViewSaveSlot,
    /// After `v g`: a digit or `d` picks the slot to promote to the global file.
    ViewGlobalSlot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pane {
    None,
    Detail,
    Help,
}

pub struct App {
    pub repo_root: PathBuf,
    pub config: Config,
    config_path: Option<PathBuf>,
    config_seen: Option<SystemTime>,
    tasks_seen: Option<SystemTime>,
    tasks: Vec<BacklogTask>,
    pub visible: Vec<usize>,
    pub selected: usize,
    /// Column order when `c m` began, so typed numbers keep meaning what the header showed.
    move_origin: Option<Vec<Column>>,
    /// Which values list to return to after a color is picked.
    paint_return: Option<Column>,
    pub views: ViewStore,
    /// Zero-based slot the current state came from.
    pub view: usize,
    /// Filter, sort, columns, glyphs, paint: what a slot saves and a restart resumes.
    pub state: ViewState,
    pub mode: Mode,
    pub input: String,
    pub pane: Pane,
    pub picker: Option<ValuePicker>,
    pub column_purpose: ColumnPurpose,
    pub status: String,
    pub last_screen: String,
    pub page_size: usize,
    pub telemetry: Telemetry,
    pub should_quit: bool,
}

impl App {
    pub fn open(
        repo_root: &Path,
        config_path: Option<PathBuf>,
        global_views_path: Option<PathBuf>,
        repo_views_path: Option<PathBuf>,
        telemetry: Telemetry,
    ) -> App {
        let config = config::load(config_path.as_deref());
        let (views, view_warnings) = ViewStore::load(global_views_path, repo_views_path);
        let mut app = App {
            repo_root: repo_root.to_path_buf(),
            config_seen: config_path.as_deref().and_then(config::modified_at),
            config_path,
            tasks_seen: None,
            tasks: Vec::new(),
            visible: Vec::new(),
            selected: 0,
            move_origin: None,
            paint_return: None,
            views,
            view: 0,
            state: ViewState::default(),
            config,
            mode: Mode::Browse,
            input: String::new(),
            pane: Pane::None,
            picker: None,
            column_purpose: ColumnPurpose::Filter,
            status: String::new(),
            last_screen: String::new(),
            page_size: 20,
            telemetry,
            should_quit: false,
        };
        app.reload_tasks();
        app.switch_view(0);
        app.report_config_warnings();
        if let Some(warning) = view_warnings.first() {
            app.fail(format!("views: {warning}"));
        }
        app
    }

    pub fn task(&self, visible_index: usize) -> Option<&BacklogTask> {
        self.visible
            .get(visible_index)
            .map(|&index| &self.tasks[index])
    }

    pub fn selected_task(&self) -> Option<&BacklogTask> {
        self.task(self.selected)
    }

    pub fn total_tasks(&self) -> usize {
        self.tasks.len()
    }

    /// The slot number while filter and sort still match it; `custom` once edited.
    /// The attributes follow in the title, so they are the name.
    pub fn view_label(&self) -> String {
        match self.views.get(self.view) {
            Some(saved) if saved == self.state => format!("v{}", self.view + 1),
            _ => "custom".to_string(),
        }
    }

    pub fn location(&self) -> String {
        let selected = self
            .selected_task()
            .map(|task| task.id.clone())
            .unwrap_or_else(|| "nothing".to_string());
        format!(
            "view={} filter=\"{}\" sort={} selected={selected} pane={:?}",
            self.view_label(),
            self.state.filter,
            self.state
                .sort
                .map(|sort| sort.to_text())
                .unwrap_or_default(),
            self.pane
        )
    }

    /// `slot\tfilter\tsort\tselected`, enough to land where the user was after a self-restart.
    pub fn resume_state(&self) -> String {
        format!("{}\t{}\t{}", self.view, self.selected, self.state.to_lua())
    }

    pub fn resume_from(&mut self, state: Option<&str>) {
        let Some(state) = state else {
            return;
        };
        let mut parts = state.splitn(3, '\t');
        if let Some(slot) = parts.next().and_then(|n| n.parse().ok()) {
            self.view = slot;
        }
        let selected = parts.next().and_then(|n| n.parse().ok());
        if let Some(record) = parts.next() {
            self.state = ViewState::from_lua(record);
        }
        self.refilter();
        if let Some(selected) = selected {
            self.select(selected);
        }
        self.status = "updated to the new build".to_string();
    }

    /// Cheap per-tick work: pick up edits to the config file or the task files.
    pub fn tick(&mut self) {
        if let Some(path) = self.config_path.as_deref() {
            let now = config::modified_at(path);
            if now != self.config_seen {
                self.config_seen = now;
                self.reload_config();
            }
        }
        let now = config::modified_at(&self.repo_root.join("backlog/tasks"));
        if now != self.tasks_seen {
            self.reload_tasks();
        }
    }

    pub fn handle_key(&mut self, event: KeyEvent) {
        if event.kind == KeyEventKind::Release {
            return;
        }
        match self.mode {
            Mode::Browse => self.handle_browse_key(event),
            Mode::Filter => self.handle_filter_key(event),
            Mode::Command => self.handle_command_key(event),
            Mode::PickValue => self.handle_pick_value_key(event),
            Mode::ViewChord => self.handle_view_chord_key(event),
            Mode::ViewSaveSlot => self.handle_view_save_slot_key(event),
            Mode::ViewGlobalSlot => self.handle_view_global_slot_key(event),
        }
    }

    fn handle_view_chord_key(&mut self, event: KeyEvent) {
        self.mode = Mode::Browse;
        match event.code {
            KeyCode::Char('s') => {
                self.mode = Mode::ViewSaveSlot;
                self.status = format!(
                    "save view into: d (default, slot 1) or 1-{}",
                    (self.views.len() + 1).min(views::MAX_SLOTS)
                );
            }
            KeyCode::Char('g') => {
                self.mode = Mode::ViewGlobalSlot;
                self.status = format!(
                    "make global: d (slot 1) or 1-{} copies that slot to every repo",
                    self.views.len()
                );
            }
            KeyCode::Char(digit) if digit.is_ascii_digit() => {
                let slot = digit.to_digit(10).unwrap_or(0) as usize;
                if (1..=self.views.len()).contains(&slot) {
                    self.switch_view(slot - 1);
                    self.telemetry.record("action", format!("view_open {slot}"));
                } else {
                    self.fail(format!(
                        "no view in slot {digit}; saved: 1-{}",
                        self.views.len()
                    ));
                }
            }
            KeyCode::Esc => self.status.clear(),
            other => {
                self.status =
                    format!("v then a slot number, s to save, or g for global, not {other:?}")
            }
        }
    }

    fn handle_view_save_slot_key(&mut self, event: KeyEvent) {
        self.mode = Mode::Browse;
        let slot = match event.code {
            KeyCode::Char('d') => 1,
            KeyCode::Char(digit) if digit.is_ascii_digit() => {
                digit.to_digit(10).unwrap_or(0) as usize
            }
            KeyCode::Esc => {
                self.status.clear();
                return;
            }
            other => {
                self.status = format!("save needs d or a slot number, not {other:?}");
                return;
            }
        };
        let next_free = self.views.len() + 1;
        if slot == 0 || slot > next_free.min(views::MAX_SLOTS) {
            self.fail(format!(
                "slot {slot} is out of reach; use 1-{}",
                next_free.min(views::MAX_SLOTS)
            ));
            return;
        }
        self.save_view(slot - 1);
    }

    fn handle_view_global_slot_key(&mut self, event: KeyEvent) {
        self.mode = Mode::Browse;
        let slot = match event.code {
            KeyCode::Char('d') => 1,
            KeyCode::Char(digit) if digit.is_ascii_digit() => {
                digit.to_digit(10).unwrap_or(0) as usize
            }
            KeyCode::Esc => {
                self.status.clear();
                return;
            }
            other => {
                self.status = format!("global needs d or a slot number, not {other:?}");
                return;
            }
        };
        if slot == 0 || slot > self.views.len() {
            self.fail(format!(
                "no view in slot {slot}; saved: 1-{}",
                self.views.len()
            ));
            return;
        }
        match self.views.promote(slot - 1) {
            Ok(()) => {
                self.status =
                    format!("slot {slot} is now global: every repo opens it with v{slot}");
                self.telemetry
                    .record("action", format!("view_global {slot}"));
            }
            Err(error) => self.fail(error),
        }
    }

    fn save_view(&mut self, slot: usize) {
        self.state.filter = self.state.filter.trim().to_string();
        let saved = self.state.clone();
        match self.views.save_repo(slot, saved) {
            Ok(()) => {
                self.view = slot;
                self.status = format!(
                    "saved v{} for this repo · vg{} makes it global",
                    slot + 1,
                    slot + 1
                );
                self.telemetry
                    .record("action", format!("view_save {}", slot + 1));
            }
            Err(error) => self.fail(error),
        }
    }

    /// After `f`/`s`: shown columns first, numbered as in the header, then hidden ones.
    fn open_column_chooser(&mut self, purpose: ColumnPurpose) {
        self.column_purpose = purpose;
        let options = self.column_picker_options();
        self.open_picker(PickerPurpose::ChooseColumn(purpose), options);
        self.status.clear();
    }

    fn open_column_purpose(&mut self, column: Column) {
        match self.column_purpose {
            ColumnPurpose::Filter => self.open_filter_picker(column),
            ColumnPurpose::Sort => self.open_sort_picker(column),
        }
    }

    /// Mirrors the header: shown columns first (so `p2` is column 2), then the
    /// lettered targets, then hidden categorical columns by name.
    fn open_paint_target_picker(&mut self) {
        let mut options: Vec<PickOption> = self
            .state
            .columns
            .iter()
            .map(|column| PickOption::column(*column, false))
            .collect();
        if let Some(task) = self.selected_task() {
            options.push(PickOption::keyed(
                'r',
                format!("row {}", task.id),
                Payload::ThisRow(task.id.clone()),
            ));
        }
        let filter = self.state.filter.trim().to_string();
        if !filter.is_empty() {
            options.push(PickOption::keyed(
                'f',
                format!("filtered rows {filter}"),
                Payload::FilteredRows(filter),
            ));
        }
        options.push(PickOption::keyed(
            'c',
            "column (whole)",
            Payload::WholeColumn,
        ));
        if !self.state.paint.is_empty() {
            options.push(PickOption::keyed(
                'o',
                format!("order rules ({})", self.state.paint.len()),
                Payload::OrderRules,
            ));
            options.push(PickOption::keyed(
                'd',
                format!("delete all paint ({} rules)", self.state.paint.len()),
                Payload::DeleteAllPaint,
            ));
        }
        for column in Column::ALL {
            if !self.state.columns.contains(&column) && column.filter_field().is_some() {
                options.push(PickOption::column(column, true));
            }
        }
        self.paint_return = None;
        self.open_picker(PickerPurpose::PaintTarget, options);
        self.telemetry.record("action", "paint");
    }

    fn is_categorical(&self, column: Column) -> bool {
        column
            .filter_field()
            .is_some_and(|field| !tasks::field_values(&self.tasks, field).is_empty())
    }

    fn open_picker(&mut self, purpose: PickerPurpose, options: Vec<PickOption>) {
        self.picker = Some(ValuePicker::new(purpose, options));
        self.mode = Mode::PickValue;
    }

    /// A column entry paints by its values when it has categories, else the whole column.
    fn paint_column_entry(&mut self, column: Column) {
        if self.is_categorical(column) {
            self.open_paint_values_picker(column);
        } else {
            self.open_paint_color_picker(PaintPick::Column(column));
        }
    }

    fn open_paint_values_picker(&mut self, column: Column) {
        let Some(field) = column.filter_field() else {
            return;
        };
        let mut options = vec![PickOption::numbered(
            "auto: one color per value",
            Payload::Auto,
        )];
        options.extend(
            tasks::field_values(&self.tasks, field)
                .into_iter()
                .map(|(value, count)| PickOption::text(value, count)),
        );
        self.paint_return = Some(column);
        self.open_picker(PickerPurpose::PaintValues(column), options);
    }

    fn open_paint_color_picker(&mut self, pick: PaintPick) {
        let mut options: Vec<PickOption> = NAMED_COLORS
            .iter()
            .map(|name| PickOption::text(*name, 0))
            .collect();
        options.push(PickOption::numbered("none", Payload::NoColor));
        self.open_picker(PickerPurpose::PaintColor(pick), options);
    }

    fn open_paint_column_picker(&mut self) {
        let options = self.column_picker_options();
        self.open_picker(PickerPurpose::PaintColumn, options);
    }

    fn open_paint_rules_picker(&mut self) {
        let options: Vec<PickOption> = self
            .state
            .paint
            .iter()
            .enumerate()
            .map(|(index, rule)| PickOption::numbered(rule.label(), Payload::Rule(index)))
            .collect();
        if options.is_empty() {
            self.status = "no paint rules".to_string();
            return;
        }
        self.open_picker(PickerPurpose::PaintRules, options);
    }

    fn paint_auto(&mut self, column: Column) {
        let Some(field) = column.filter_field() else {
            return;
        };
        for (index, (value, _)) in tasks::field_values(&self.tasks, field).iter().enumerate() {
            let color = paint::AUTO_PALETTE[index % paint::AUTO_PALETTE.len()];
            paint::set_value_color(&mut self.state.paint, column, value, Some(color));
        }
        self.status = format!("painted every {} value", column.name());
        self.telemetry
            .record("action", format!("paint_auto {}", column.name()));
    }

    /// `b`: nobody → me → agent → nobody on the selected task, written as a label.
    fn pass_ball(&mut self) {
        let Some(task) = self.selected_task() else {
            self.status = "no task selected".to_string();
            return;
        };
        let (id, current) = (task.id.clone(), Ball::of(task));
        let next = Ball::next(current);
        let mut outcome = Ok(String::new());
        if let Some(old) = current {
            outcome =
                switchbard_core::set_backlog_label(&self.repo_root, &id, Ball::label(old), false);
        }
        if let (Ok(_), Some(new)) = (&outcome, next) {
            outcome =
                switchbard_core::set_backlog_label(&self.repo_root, &id, Ball::label(new), true);
        }
        match outcome {
            Ok(_) => {
                self.status = match next {
                    Some(ball) => format!("{id}: ball → {}", Ball::text(Some(ball))),
                    None => format!("{id}: ball dropped"),
                };
                self.telemetry
                    .record("action", format!("ball {}", Ball::text(next)));
                self.reload_tasks();
            }
            Err(error) => self.fail(format!("{id}: {error}")),
        }
    }

    fn clear_all_paint(&mut self) {
        let count = self.state.paint.len();
        self.state.paint.clear();
        self.status = format!("deleted {count} paint rules");
        self.telemetry.record("action", "paint_clear_all");
    }

    fn apply_paint(&mut self, pick: PaintPick, color: &str) {
        let cleared = color == "none";
        match &pick {
            PaintPick::Value(column, value) => paint::set_value_color(
                &mut self.state.paint,
                *column,
                value,
                (!cleared).then_some(color),
            ),
            PaintPick::Rows(filter) => paint::set_rule(
                &mut self.state.paint,
                PaintRule::Rows {
                    filter: filter.clone(),
                    color: color.to_string(),
                },
            ),
            PaintPick::Column(column) => paint::set_rule(
                &mut self.state.paint,
                PaintRule::Column {
                    column: *column,
                    color: color.to_string(),
                },
            ),
        }
        self.status = if cleared {
            "paint cleared".to_string()
        } else {
            format!("painted {color}")
        };
        self.telemetry
            .record("action", format!("paint_apply {color}"));
        if let Some(column) = self.paint_return {
            self.open_paint_values_picker(column);
        }
    }

    /// `h`/Left inside a paint flow: one level up, back to the target list.
    fn paint_back(&mut self) {
        self.paint_return = None;
        self.open_paint_target_picker();
    }

    fn move_paint_rule(&mut self, index: usize, delta: isize) -> usize {
        let target = index as isize + delta;
        if index < self.state.paint.len()
            && target >= 0
            && (target as usize) < self.state.paint.len()
        {
            self.state.paint.swap(index, target as usize);
            self.telemetry.record("action", "paint_reorder");
            return target as usize;
        }
        index
    }

    fn open_columns_picker(&mut self) {
        let options = self.column_picker_options();
        self.open_picker(PickerPurpose::Columns, options);
        self.telemetry.record("action", "columns");
    }

    /// Shown columns first in display order, then the hidden ones.
    fn column_picker_options(&self) -> Vec<PickOption> {
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

    fn toggle_column(&mut self, column: Column) {
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

    fn shown_column_options(&self) -> Vec<PickOption> {
        self.state
            .columns
            .iter()
            .map(|column| PickOption::column(*column, false))
            .collect()
    }

    /// `c m 3 1 2`: the columns numbered by `placed` (as the header showed them
    /// when `m` was pressed) come first in that order; the rest keep theirs.
    fn reorder_columns(&mut self, placed: &[usize]) {
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
        values.sort_by_key(|value| (sort::vocabulary_rank(column, value), value.clone()));
        values
            .iter()
            .map(|value| self.config.glyph(column, value))
            .collect()
    }

    /// `g` in the columns picker: glyphs instead of text, for categorical columns.
    fn toggle_glyph_column(&mut self, column: Column) {
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

    fn move_column(&mut self, column: Column, delta: isize) {
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

    fn open_filter_picker(&mut self, column: Column) {
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

    fn open_sort_picker(&mut self, column: Column) {
        let mut options: Vec<PickOption> = sort::orders_for(column)
            .into_iter()
            .map(|order| PickOption::numbered(order.label(column), Payload::Order(order)))
            .collect();
        options.push(PickOption::numbered("none", Payload::NoSort));
        self.open_picker(PickerPurpose::Sort(column), options);
        self.telemetry
            .record("action", format!("sort_column {}", column.header()));
    }

    fn handle_pick_value_key(&mut self, event: KeyEvent) {
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

    fn toggle_picked_value(&mut self) {
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

    fn toggle_filter_value(&mut self, field: FilterField, value: &str) {
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

    fn apply_picked_value(&mut self) {
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

    /// Commands that start with what has been typed so far, for the footer hint.
    pub fn command_completions(&self) -> Vec<String> {
        let typed = self.input.split_whitespace().next().unwrap_or("");
        if self.input.contains(' ') {
            return Vec::new();
        }
        let mut names: Vec<String> = ["bug", "idea", "reload", "q"]
            .iter()
            .map(|name| name.to_string())
            .collect();
        names.retain(|name| name.starts_with(typed) && name != typed);
        names
    }

    fn handle_browse_key(&mut self, event: KeyEvent) {
        let chord = KeyChord::from_event(&event);
        match self.config.keys.get(&chord).cloned() {
            Some(action) => {
                let started = Instant::now();
                self.apply(&action);
                self.telemetry
                    .record_timed("action", action.name(), started);
            }
            None => {
                self.telemetry.record("unbound", chord.label());
                self.status = format!("{} is not bound. ? lists keys", chord.label());
            }
        }
    }

    fn handle_filter_key(&mut self, event: KeyEvent) {
        match event.code {
            KeyCode::Esc => {
                self.mode = Mode::Browse;
                self.telemetry.record("action", "filter_cancel");
            }
            KeyCode::Enter => {
                self.mode = Mode::Browse;
                self.telemetry
                    .record("action", format!("filter_apply {}", self.state.filter));
            }
            KeyCode::Backspace => {
                self.state.filter.pop();
                self.refilter();
            }
            KeyCode::Char(c) => {
                self.state.filter.push(c);
                self.refilter();
            }
            _ => {}
        }
    }

    fn handle_command_key(&mut self, event: KeyEvent) {
        match event.code {
            KeyCode::Esc => {
                self.mode = Mode::Browse;
                self.input.clear();
            }
            KeyCode::Tab => {
                if let Some(first) = self.command_completions().first() {
                    self.input = first.clone();
                }
            }
            KeyCode::Enter => {
                self.mode = Mode::Browse;
                let command = std::mem::take(&mut self.input);
                let started = Instant::now();
                self.run_command(command.trim());
                self.telemetry.record_timed(
                    "action",
                    format!(
                        "command {}",
                        command.split_whitespace().next().unwrap_or("")
                    ),
                    started,
                );
            }
            KeyCode::Backspace => {
                self.input.pop();
            }
            KeyCode::Char(c) => self.input.push(c),
            _ => {}
        }
    }

    fn apply(&mut self, action: &Action) {
        match action {
            Action::Down => self.select(self.selected.saturating_add(1)),
            Action::Up => self.select(self.selected.saturating_sub(1)),
            Action::Top => self.select(0),
            Action::Bottom => self.select(usize::MAX),
            Action::PageDown => self.select(self.selected.saturating_add(self.page_size)),
            Action::PageUp => self.select(self.selected.saturating_sub(self.page_size)),
            Action::Open => {
                self.pane = match self.pane {
                    Pane::Detail => Pane::None,
                    _ => Pane::Detail,
                }
            }
            Action::Back => {
                if self.pane != Pane::None {
                    self.pane = Pane::None;
                } else if !self.state.filter.is_empty() {
                    self.set_filter(String::new());
                }
                self.status.clear();
            }
            Action::Filter => {
                self.mode = Mode::Filter;
                self.status.clear();
                if !self.state.filter.is_empty() && !self.state.filter.ends_with(' ') {
                    self.state.filter.push(' ');
                }
            }
            Action::FilterColumn => self.open_column_chooser(ColumnPurpose::Filter),
            Action::SortColumn => self.open_column_chooser(ColumnPurpose::Sort),
            Action::Columns => self.open_columns_picker(),
            Action::Paint => self.open_paint_target_picker(),
            Action::Ball => self.pass_ball(),
            Action::Command => {
                self.mode = Mode::Command;
                self.input.clear();
                self.status.clear();
            }
            Action::Reload => {
                self.reload_config();
                self.reload_tasks();
                self.status = format!("reloaded {} tasks", self.tasks.len());
            }
            Action::Help => {
                self.pane = match self.pane {
                    Pane::Help => Pane::None,
                    _ => Pane::Help,
                }
            }
            Action::Quit => self.should_quit = true,
            Action::View => {
                self.mode = Mode::ViewChord;
                self.status = format!(
                    "view: 1-{} opens a slot · s saves · g makes global",
                    self.views.len()
                );
            }
        }
    }

    fn run_command(&mut self, command: &str) {
        let (verb, rest) = command.split_once(' ').unwrap_or((command, ""));
        match verb {
            "q" | "quit" => self.should_quit = true,
            "reload" => self.apply(&Action::Reload),
            "bug" => self.file_report(ReportKind::Bug, rest),
            "idea" => self.file_report(ReportKind::Idea, rest),
            "" => {}
            other => self.fail(format!("unknown command :{other}")),
        }
    }

    fn file_report(&mut self, kind: ReportKind, intent: &str) {
        let location = self.location();
        let trail = self.telemetry.trail();
        let context = ReportContext {
            intent,
            location: &location,
            screen: &self.last_screen,
            trail: &trail,
        };
        match report::file_report(&self.repo_root, kind, context) {
            Ok(bare_id) => {
                self.reload_tasks();
                let filed = self
                    .tasks
                    .iter()
                    .position(|task| task.id.rsplit('-').next() == Some(bare_id.as_str()));
                let shown_id = filed
                    .map(|index| self.tasks[index].id.clone())
                    .unwrap_or(bare_id);
                if let Some(visible_index) = filed.and_then(|index| {
                    self.visible
                        .iter()
                        .position(|&candidate| candidate == index)
                }) {
                    self.select(visible_index);
                }
                self.status = format!("filed {shown_id}");
                self.telemetry
                    .record("report", format!("{kind:?} {shown_id}"));
            }
            Err(error) => self.fail(error.to_string()),
        }
    }

    fn switch_view(&mut self, slot: usize) {
        let Some(saved) = self.views.get(slot) else {
            return;
        };
        self.view = slot;
        self.state = saved;
        self.refilter();
    }

    fn set_filter(&mut self, text: String) {
        self.state.filter = text;
        self.refilter();
    }

    fn refilter(&mut self) {
        let filter = Filter::parse(&self.state.filter);
        self.visible = (0..self.tasks.len())
            .filter(|&index| filter.matches(&self.tasks[index]))
            .collect();
        if let Some(sort) = self.state.sort {
            sort::apply(&self.tasks, &mut self.visible, sort);
        }
        self.select(self.selected);
    }

    fn select(&mut self, index: usize) {
        self.selected = index.min(self.visible.len().saturating_sub(1));
    }

    fn reload_tasks(&mut self) {
        self.tasks_seen = config::modified_at(&self.repo_root.join("backlog/tasks"));
        match tasks::load(&self.repo_root) {
            Ok(tasks) => self.tasks = tasks,
            Err(error) => self.fail(error.to_string()),
        }
        self.refilter();
    }

    fn reload_config(&mut self) {
        self.config = config::load(self.config_path.as_deref());
        self.status = "config reloaded".to_string();
        self.telemetry
            .record("config_reload", self.config.warnings.len().to_string());
        self.report_config_warnings();
    }

    fn report_config_warnings(&mut self) {
        if let Some(first) = self.config.warnings.first() {
            self.fail(format!("config: {first}"));
        }
    }

    fn fail(&mut self, message: String) {
        self.telemetry.record("error", message.clone());
        self.status = message;
    }
}
