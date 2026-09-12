//! Column, filter, and sort pickers, and the one key handler every picker shares.

use std::str::FromStr;

use crossterm::event::{KeyCode, KeyEvent};

use crate::app::{App, Mode};
use crate::ball::Ball;
use crate::columns::Column;
use crate::picker::{
    ColumnAction, ColumnPurpose, PaintPick, Payload, PickOption, PickerPurpose, TaskAction,
    ValuePicker,
};
use crate::sort::{self, Sort};
use crate::tasks::{self, Filter, FilterField};

impl App {
    pub(super) fn open_task_picker(&mut self) {
        let mut options = vec![PickOption::keyed(
            'n',
            "New task",
            Payload::TaskAction(TaskAction::New),
        )];
        if self.selected_task().is_some() {
            for (key, label, action) in [
                ('b', "Assign ball", TaskAction::Ball),
                ('s', "Status", TaskAction::Status),
                ('d', "Mark Done", TaskAction::Done),
                ('p', "Link project", TaskAction::Project),
                ('a', "Link parent task", TaskAction::Parent),
                ('r', "Top list", TaskAction::TopList),
                ('g', "Link goals", TaskAction::Goals),
            ] {
                options.push(PickOption::keyed(key, label, Payload::TaskAction(action)));
            }
        }
        self.open_picker(PickerPurpose::Task, options);
        self.status.clear();
    }

    fn open_top_list_picker(&mut self) {
        let mut options = vec![
            PickOption::keyed(
                'a',
                "Add or move task to end",
                Payload::TaskAction(TaskAction::Append),
            ),
            PickOption::keyed(
                'x',
                "Remove task from top list",
                Payload::TaskAction(TaskAction::Drop),
            ),
        ];
        options.extend(
            (1..=self.top.len().saturating_add(1).min(10000)).map(|rank| {
                PickOption::numbered(format!("Move task to position {rank}"), Payload::Rank(rank))
            }),
        );
        self.open_picker(PickerPurpose::TopList, options);
    }

    fn run_task_action(&mut self, action: TaskAction) {
        match action {
            TaskAction::New => self.open_new_task(),
            TaskAction::Ball => self.open_ball_picker(),
            TaskAction::Append => self.set_rank(self.top.len() + 1),
            TaskAction::Status => self.open_task_status_picker(),
            TaskAction::Project => self.open_task_project_picker(),
            TaskAction::Parent => self.open_task_parent_picker(),
            TaskAction::TopList => self.open_top_list_picker(),
            TaskAction::Drop => self.drop_rank(),
            TaskAction::Goals => self.open_goal_picker(),
            TaskAction::Done => self.mark_done(),
            TaskAction::Pin => {
                let selected = self.selected_task().map(|task| task.id.clone());
                self.state.pin_top = !self.state.pin_top;
                self.refilter();
                if let Some(id) = selected {
                    self.select_task(&id);
                }
                self.status = if self.state.pin_top {
                    "top list pinned first"
                } else {
                    "top list unpinned"
                }
                .to_string();
                self.telemetry
                    .record("action", format!("pin_top {}", self.state.pin_top));
            }
        }
    }

    fn columns_menu_options(&self) -> Vec<PickOption> {
        let mut options = self.column_picker_options();
        options.extend(
            [
                ColumnAction::Move,
                ColumnAction::Glyphs,
                ColumnAction::Abbreviate,
                ColumnAction::Earlier,
                ColumnAction::Later,
            ]
            .into_iter()
            .filter(|action| {
                self.page == crate::page::Page::Tasks || *action != ColumnAction::Abbreviate
            })
            .map(|action| {
                PickOption::keyed(action.key(), action.label(), Payload::ColumnAction(action))
            }),
        );
        options
    }

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

    /// A digit in browse: the column at that header position and what can be done with it.
    pub(super) fn open_column_actions(&mut self, position: usize) {
        let Some(column) = self.state.columns.get(position - 1).copied() else {
            self.status = format!("no column {position}: the header numbers the shown ones");
            return;
        };
        let actions = [
            ColumnAction::Filter,
            ColumnAction::Sort,
            ColumnAction::Group,
            ColumnAction::Paint,
            ColumnAction::Glyphs,
            ColumnAction::Abbreviate,
            ColumnAction::Hide,
            ColumnAction::Move,
        ];
        let options = actions
            .into_iter()
            .filter(|action| *action != ColumnAction::Glyphs || self.is_categorical(column))
            .filter(|action| {
                *action != ColumnAction::Group
                    || (self.page == crate::page::Page::Tasks && column.groupable())
            })
            .filter(|action| {
                *action != ColumnAction::Abbreviate
                    || (self.page == crate::page::Page::Tasks && column.abbreviable())
            })
            .map(|action| {
                PickOption::keyed(action.key(), action.label(), Payload::ColumnAction(action))
            })
            .collect();
        self.open_picker(PickerPurpose::ColumnActions(column), options);
        self.status.clear();
        self.telemetry
            .record("action", format!("column_actions {}", column.name()));
    }

    pub(super) fn run_column_action(&mut self, column: Column, action: ColumnAction) {
        match action {
            ColumnAction::Filter => self.open_filter_picker(column),
            ColumnAction::Sort => self.open_sort_picker(column),
            ColumnAction::Group => self.set_group(crate::group::Grouping::by(column)),
            ColumnAction::Paint => self.paint_column_entry(column),
            ColumnAction::Glyphs => self.toggle_glyph_column(column),
            ColumnAction::Abbreviate => self.toggle_abbreviated(column),
            ColumnAction::Hide => self.toggle_column(column),
            ColumnAction::Earlier => self.move_column(column, -1),
            ColumnAction::Later => self.move_column(column, 1),
            ColumnAction::Move => {
                self.move_origin = Some(self.state.columns.clone());
                let options = self.shown_column_options();
                self.open_picker(PickerPurpose::MoveColumns(Vec::new()), options);
            }
        }
    }

    pub(super) fn open_picker(&mut self, purpose: PickerPurpose, options: Vec<PickOption>) {
        if let Some(parent) = self.picker.take() {
            self.remember_picker_parent(parent, &purpose);
        }
        self.picker = Some(ValuePicker::new(purpose, options));
        self.mode = Mode::PickValue;
    }

    fn remember_picker_parent(&mut self, parent: ValuePicker, next: &PickerPurpose) {
        if parent.purpose == *next {
            return;
        }
        if let Some(index) = self
            .picker_parents
            .iter()
            .position(|picker| picker.purpose == *next)
        {
            self.picker_parents.truncate(index);
        } else {
            if self.picker_parents.len() == 16 {
                self.picker_parents.remove(0);
            }
            self.picker_parents.push(parent);
        }
    }

    fn picker_back(&mut self) {
        self.picker = self.picker_parents.pop();
        self.paint_return = match self.picker.as_ref().map(|picker| &picker.purpose) {
            Some(PickerPurpose::PaintValues(column)) => Some(*column),
            _ => None,
        };
        self.mode = if self.picker.is_some() {
            Mode::PickValue
        } else {
            Mode::Browse
        };
        self.status.clear();
    }

    pub(super) fn open_columns_picker(&mut self) {
        let options = self.columns_menu_options();
        self.open_picker(PickerPurpose::Columns, options);
        self.telemetry.record("action", "columns");
    }

    /// `t b`: choose a standard holder, a person already named in this repo,
    /// or the entry path for someone new.
    pub(super) fn open_ball_picker(&mut self) {
        let mut people = self
            .tasks
            .iter()
            .filter_map(|task| match Ball::of(task) {
                Some(Ball::Other(person)) => Some(person),
                _ => None,
            })
            .collect::<Vec<_>>();
        people.sort_unstable();
        people.dedup();
        let mut options = vec![
            PickOption::numbered("me", Payload::Ball(Some(Ball::Me))),
            PickOption::numbered("agent", Payload::Ball(Some(Ball::Agent))),
            PickOption::numbered("none · drop", Payload::Ball(None)),
        ];
        options.extend(people.into_iter().map(|person| {
            PickOption::numbered(person.clone(), Payload::Ball(Some(Ball::Other(person))))
        }));
        options.push(PickOption::numbered("new person…", Payload::NewBallHolder));
        self.open_picker(PickerPurpose::Ball, options);
        self.status.clear();
        self.telemetry.record("action", "ball_picker");
    }

    /// Shown columns first in display order, then the hidden ones.
    pub(super) fn column_picker_options(&self) -> Vec<PickOption> {
        self.state
            .columns
            .iter()
            .map(|column| PickOption::column(*column, false))
            .chain(
                self.page_columns()
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
        let mut values: Vec<String> = tasks::field_values(&self.tasks, field, &self.goals)
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

    /// `1a` or `a` in the columns picker: short form (bare id, H/M/L) on or off.
    pub(super) fn toggle_abbreviated(&mut self, column: Column) {
        if self.page == crate::page::Page::PullRequests || !column.abbreviable() {
            self.status = format!("{} has no short form", column.name());
            return;
        }
        match self.state.abbreviated.iter().position(|c| *c == column) {
            Some(index) => {
                self.state.abbreviated.remove(index);
                self.status = format!("{} shows in full", column.name());
            }
            None => {
                self.state.abbreviated.push(column);
                self.status = format!("{} abbreviated", column.name());
            }
        }
        self.telemetry
            .record("action", format!("abbreviate_toggle {}", column.name()));
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

    pub(super) fn column_values(&self, column: Column) -> Vec<(String, usize)> {
        let mut values = if self.page == crate::page::Page::PullRequests {
            self.pull_requests.column_values(column)
        } else {
            column
                .filter_field()
                .map(|field| tasks::field_values(&self.tasks, field, &self.goals))
                .unwrap_or_default()
        };
        if column.is_date() {
            for bucket in column.spec().vocabulary {
                if !values.iter().any(|(value, _)| value == bucket) {
                    values.push(((*bucket).to_string(), 0));
                }
            }
            values.sort_by_key(|(value, _)| column.vocabulary_rank(value));
        }
        values
    }

    pub(super) fn open_filter_picker(&mut self, column: Column) {
        let field = if self.page == crate::page::Page::PullRequests && column == Column::Id {
            Some(FilterField::Id)
        } else {
            column.filter_field()
        };
        match field {
            Some(field) => {
                let values = self.column_values(column);
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
        if self
            .picker
            .as_ref()
            .is_some_and(|p| p.purpose == PickerPurpose::Merge)
        {
            self.handle_merge_picker_key(event);
            return;
        }
        if self.handle_parent_search_key(event) {
            return;
        }
        let task_rank_room = self
            .selected_task()
            .map(|_| self.top.len().saturating_add(1))
            .unwrap_or(0);
        let Some(picker) = self.picker.as_mut() else {
            self.mode = Mode::Browse;
            return;
        };
        let mut event = event;
        let legacy_value_initial = matches!(picker.purpose, PickerPurpose::Filter(_))
            && matches!(event.code, KeyCode::Char('h' | 'l'))
            && picker.options.iter().any(|option| {
                let KeyCode::Char(letter) = event.code else {
                    return false;
                };
                Filter::loose_starts_with(&option.label, &letter.to_string())
            });
        if event.code == KeyCode::Left
            || (event.code == KeyCode::Char('h')
                && picker.typed.is_empty()
                && !legacy_value_initial)
        {
            self.picker_back();
            return;
        }
        if event.code == KeyCode::Right
            || (event.code == KeyCode::Char('l')
                && picker.typed.is_empty()
                && !legacy_value_initial)
        {
            event.code = KeyCode::Enter;
        }
        let last = picker.matching().len().saturating_sub(1);
        let purpose = picker.purpose.clone();
        let typed_empty = picker.typed.is_empty();
        match event.code {
            KeyCode::Esc => {
                self.picker = None;
                self.picker_parents.clear();
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
            KeyCode::Char('t') if purpose == PickerPurpose::Task && typed_empty => {
                self.picker = None;
                self.mode = Mode::Browse;
                self.run_task_action(TaskAction::Append);
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
                let count = if matches!(purpose, PickerPurpose::Task | PickerPurpose::TopList) {
                    task_rank_room
                } else if matches!(
                    purpose,
                    PickerPurpose::Views
                        | PickerPurpose::SaveView
                        | PickerPurpose::GlobalView
                        | PickerPurpose::RenameView
                        | PickerPurpose::DeleteView
                ) {
                    picker
                        .row_keys()
                        .iter()
                        .filter_map(|key| key.parse::<usize>().ok())
                        .max()
                        .unwrap_or(0)
                } else {
                    picker.numbered_count()
                };
                let index = if matches!(purpose, PickerPurpose::Task | PickerPurpose::TopList)
                    && count > 0
                {
                    index.min(count)
                } else {
                    index
                };
                let could_extend = index.saturating_mul(10) <= count;
                if index == 0 || index > count {
                    picker.number.clear();
                    self.status = match purpose {
                        PickerPurpose::Views
                        | PickerPurpose::GlobalView
                        | PickerPurpose::RenameView
                        | PickerPurpose::DeleteView => {
                            format!("no view in slot {index}")
                        }
                        PickerPurpose::SaveView => format!("slot {index} is out of reach"),
                        PickerPurpose::Task if index == 0 => "rank: 1 is the top".to_string(),
                        _ => format!("no choice {index}"),
                    };
                } else if could_extend {
                    // Wait: a second digit may still follow (1 when there are 10+).
                    if matches!(purpose, PickerPurpose::Task | PickerPurpose::TopList) {
                        self.status = format!("rank: {index}▏ (another digit, or enter)");
                    }
                } else {
                    picker.number.clear();
                    if matches!(purpose, PickerPurpose::Task | PickerPurpose::TopList) {
                        self.picker = None;
                        self.mode = Mode::Browse;
                        self.set_rank(index);
                    } else if let Some(position) = picker.position_of_number(index) {
                        picker.selected = position;
                        self.apply_picked_value();
                    } else if matches!(
                        purpose,
                        PickerPurpose::Views
                            | PickerPurpose::GlobalView
                            | PickerPurpose::RenameView
                            | PickerPurpose::DeleteView
                    ) {
                        self.status = format!("no view in slot {index}");
                    } else if purpose == PickerPurpose::SaveView {
                        self.status = format!("slot {index} is out of reach");
                    }
                }
            }
            KeyCode::Enter if !picker.number.is_empty() => {
                let index: usize = picker.number.parse().unwrap_or(0);
                picker.number.clear();
                if matches!(purpose, PickerPurpose::Task | PickerPurpose::TopList)
                    && task_rank_room > 0
                {
                    self.picker = None;
                    self.mode = Mode::Browse;
                    self.set_rank(index.max(1).min(task_rank_room));
                } else if let Some(position) = picker.position_of_number(index) {
                    picker.selected = position;
                    self.apply_picked_value();
                }
            }
            KeyCode::Enter if matches!(purpose, PickerPurpose::MoveColumns(_)) => {
                self.move_origin = None;
                let options = self.columns_menu_options();
                self.open_picker(PickerPurpose::Columns, options);
            }
            KeyCode::Enter => self.apply_picked_value(),
            KeyCode::Delete | KeyCode::Backspace
                if matches!(purpose, PickerPurpose::Task | PickerPurpose::TopList)
                    && typed_empty =>
            {
                self.picker = None;
                self.mode = Mode::Browse;
                self.drop_rank();
            }
            KeyCode::Delete | KeyCode::Backspace
                if purpose == PickerPurpose::PaintTarget && typed_empty =>
            {
                self.picker = None;
                self.mode = Mode::Browse;
                self.clear_all_paint();
            }
            KeyCode::Delete | KeyCode::Backspace
                if purpose == PickerPurpose::PaintRules
                    && typed_empty
                    && matches!(
                        picker.highlighted().map(|o| o.payload),
                        Some(Payload::Rule(_))
                    ) =>
            {
                let Some(Payload::Rule(index)) = picker.highlighted().map(|o| o.payload) else {
                    return;
                };
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
            KeyCode::Char(direction @ ('J' | 'K'))
                if purpose == PickerPurpose::PaintRules
                    && matches!(
                        picker.highlighted().map(|o| o.payload),
                        Some(Payload::Rule(_))
                    ) =>
            {
                let delta = if direction == 'J' { 1 } else { -1 };
                let Some(Payload::Rule(selected)) = picker.highlighted().map(|o| o.payload) else {
                    return;
                };
                let moved_to = self.move_paint_rule(selected, delta);
                self.open_paint_rules_picker();
                if let Some(picker) = self.picker.as_mut() {
                    picker.selected = moved_to;
                }
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
            KeyCode::Char('g')
                if purpose == PickerPurpose::Columns
                    && typed_empty
                    && matches!(
                        picker.highlighted().map(|o| o.payload),
                        Some(Payload::Column(_))
                    ) =>
            {
                if let Some(Payload::Column(column)) = picker.highlighted().map(|o| o.payload) {
                    self.toggle_glyph_column(column);
                }
            }
            KeyCode::Char('a')
                if purpose == PickerPurpose::Columns
                    && typed_empty
                    && matches!(
                        picker.highlighted().map(|o| o.payload),
                        Some(Payload::Column(_))
                    ) =>
            {
                if let Some(Payload::Column(column)) = picker.highlighted().map(|o| o.payload) {
                    self.toggle_abbreviated(column);
                }
            }
            KeyCode::Char(direction @ ('J' | 'K'))
                if purpose == PickerPurpose::Columns
                    && matches!(
                        picker.highlighted().map(|o| o.payload),
                        Some(Payload::Column(_))
                    ) =>
            {
                if let Some(Payload::Column(column)) = picker.highlighted().map(|o| o.payload) {
                    let delta = if direction == 'J' { 1 } else { -1 };
                    let moved_to = picker.selected as isize + delta;
                    self.move_column(column, delta);
                    let options = self.columns_menu_options();
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
                if picker.typed.chars().count() >= 256 {
                    return;
                }
                picker.typed.push(c);
                picker.selected = 0;
                // A toggle panel must not flip on a unique match: typing the
                // rest of the word would flip it back. Enter or a number commits.
                let toggles = matches!(
                    purpose,
                    PickerPurpose::Settings
                        | PickerPurpose::Goals(_)
                        | PickerPurpose::Task
                        | PickerPurpose::TaskStatus(_)
                        | PickerPurpose::TaskProject(_)
                        | PickerPurpose::TopList
                        | PickerPurpose::Views
                        | PickerPurpose::SaveView
                        | PickerPurpose::GlobalView
                        | PickerPurpose::Columns
                        | PickerPurpose::ChooseColumnAction(_)
                        | PickerPurpose::PaintRules
                        | PickerPurpose::ChoosePaintRule(_)
                );
                let matches = picker.matching();
                let legacy_column_pick = purpose == PickerPurpose::Columns
                    && matches
                        .first()
                        .is_some_and(|option| matches!(option.payload, Payload::Column(_)));
                if matches.len() == 1 && (!toggles || legacy_column_pick) {
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
                let options = self.columns_menu_options();
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
            .filter(|candidate| Filter::field_allows(self.filter_text(), field, candidate))
            .cloned()
            .collect();
        match shown.iter().position(|candidate| candidate == value) {
            Some(index) => {
                shown.remove(index);
            }
            None => shown.push(value.to_string()),
        }
        let text = Filter::with_shown(self.filter_text(), field, &all, &shown);
        self.set_filter(text);
        self.telemetry.record(
            "action",
            format!("filter_toggle {}:{value}", field.keyword()),
        );
    }

    pub(super) fn apply_picked_value(&mut self) {
        if let Some(picker) = self.picker.as_ref().filter(|picker| {
            matches!(picker.purpose, PickerPurpose::TaskParent(_)) && picker.highlighted().is_none()
        }) {
            self.status = format!(
                "nothing matches '{}'; edit search or Esc cancels",
                picker.typed
            );
            return;
        }
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
        let parent = picker.clone();
        match (picker.purpose, picked.payload) {
            (PickerPurpose::PaintRules, Payload::PaintRuleAction(action)) => {
                self.open_paint_rule_action(action)
            }
            (PickerPurpose::ChoosePaintRule(action), Payload::Rule(index)) => {
                self.run_paint_rule_action(index, action)
            }
            (
                PickerPurpose::Task | PickerPurpose::TopList | PickerPurpose::Views,
                Payload::TaskAction(action),
            ) => self.run_task_action(action),
            (PickerPurpose::TaskParent(id), Payload::Parent(parent)) => {
                self.change_task_parent(&id, parent.as_deref())
            }
            (PickerPurpose::TaskProject(id), Payload::Project(project)) => {
                self.change_task_project(&id, project.as_deref())
            }
            (PickerPurpose::TaskStatus(id), Payload::Text(status)) => {
                self.change_task_status(&id, &status)
            }
            (PickerPurpose::Task | PickerPurpose::TopList, Payload::Rank(rank)) => {
                self.set_rank(rank)
            }
            (PickerPurpose::Views, Payload::ViewSlot(slot)) => {
                self.switch_view(slot);
                self.telemetry
                    .record("action", format!("view_open {}", slot + 1));
            }
            (PickerPurpose::Views, Payload::SaveView) => {
                self.open_view_picker(PickerPurpose::SaveView)
            }
            (PickerPurpose::Views, Payload::GlobalView) => {
                self.open_view_picker(PickerPurpose::GlobalView)
            }
            (PickerPurpose::Views, Payload::RenameView) => {
                self.open_view_picker(PickerPurpose::RenameView)
            }
            (PickerPurpose::Views, Payload::DeleteView) => {
                self.open_view_picker(PickerPurpose::DeleteView)
            }
            (PickerPurpose::RenameView, Payload::ViewSlot(slot)) => self.open_rename_view(slot),
            (PickerPurpose::DeleteView, Payload::ViewSlot(slot)) => self.delete_view(slot),
            (PickerPurpose::SaveView, Payload::ViewSlot(slot)) => self.save_view(slot),
            (PickerPurpose::GlobalView, Payload::ViewSlot(slot)) => self.promote_view(slot),
            (PickerPurpose::Settings, Payload::GlobalSettings) => self.promote_settings(),
            (PickerPurpose::Settings, Payload::TitleWrapping) => self.cycle_row_layout(true),
            (PickerPurpose::Settings, Payload::RowSpacing) => self.cycle_row_layout(false),
            (PickerPurpose::Columns, Payload::ColumnAction(ColumnAction::Move)) => {
                self.move_origin = Some(self.state.columns.clone());
                self.open_picker(
                    PickerPurpose::MoveColumns(Vec::new()),
                    self.shown_column_options(),
                );
            }
            (PickerPurpose::Columns, Payload::ColumnAction(action)) => {
                let options = self.column_picker_options();
                self.open_picker(PickerPurpose::ChooseColumnAction(action), options);
            }
            (PickerPurpose::ChooseColumnAction(action), Payload::Column(column)) => {
                self.run_column_action(column, action);
                self.open_columns_picker();
            }

            (PickerPurpose::Merge, Payload::CancelMerge) => self.cancel_pr_merge(),
            (PickerPurpose::Merge, Payload::Merge(method)) => self.submit_pr_merge(method),
            (PickerPurpose::Filter(field), Payload::Text(value)) => {
                let text = Filter::with_only(self.filter_text(), field, &value);
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
                    let options = self.columns_menu_options();
                    let highlight = self.state.columns.len().saturating_sub(1);
                    self.open_picker(PickerPurpose::Columns, options);
                    if let Some(picker) = self.picker.as_mut() {
                        picker.selected = highlight;
                    }
                    self.status = format!(
                        "{} added as column {}",
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
            (PickerPurpose::Settings, Payload::Text(status)) => {
                // apply_picked_value took the picker; toggle_setting reopens it.
                self.toggle_setting(&status)
            }
            (PickerPurpose::Organize, Payload::Grouping(grouping)) => {
                if self.state.group == grouping && !grouping.is_flat() {
                    self.set_group(crate::group::Grouping::flat())
                } else {
                    self.set_group(grouping)
                }
            }
            (PickerPurpose::Goals(_), Payload::Text(name)) => {
                // Same shape as settings: the panel stays open with fresh marks.
                self.open_goal_picker();
                self.toggle_goal_link(&name)
            }
            (PickerPurpose::Ball, Payload::Ball(ball)) => self.assign_ball(ball),
            (PickerPurpose::Ball, Payload::NewBallHolder) => {
                self.mode = Mode::BallName;
                self.input.clear();
            }
            (PickerPurpose::ColumnActions(column), Payload::ColumnAction(action)) => {
                self.run_column_action(column, action)
            }
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
        if let Some(next) = self.picker.as_ref().map(|picker| picker.purpose.clone()) {
            self.remember_picker_parent(parent, &next);
        }
    }
}
