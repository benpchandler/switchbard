//! Application state and the single place key events turn into state changes.

use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::time::{Instant, SystemTime};

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind};
use switchbard_core::BacklogTask;

use crate::config::{self, Action, Column, Config, KeyChord};
use crate::paint::{self, parse_rules, rules_text, PaintRule, PaintTarget, NAMED_COLORS};
use crate::report::{self, ReportContext, ReportKind};
use crate::sort::{self, Sort};
use crate::tasks::{self, Filter, FilterField};
use crate::telemetry::Telemetry;
use crate::views::{self, columns_text, parse_columns, SavedView, ViewStore};

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

/// What a column was picked for: `f` filters by its values, `s` sorts by it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColumnPurpose {
    Filter,
    Sort,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PickerPurpose {
    Filter(FilterField),
    Sort(Column),
    /// Typing a column name after `f`/`s`, so hidden columns stay reachable.
    ChooseColumn(ColumnPurpose),
    /// The `c` picker: which columns show, in what order.
    Columns,
    /// After `p`: what to paint.
    PaintTarget,
    /// After a target: which color.
    PaintColor(PaintTarget),
}

/// The `f`/`s <column>` picker: its options, the typed narrowing text, and the cursor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValuePicker {
    pub purpose: PickerPurpose,
    pub options: Vec<(String, usize)>,
    pub typed: String,
    pub selected: usize,
}

impl ValuePicker {
    /// Options starting with what has been typed; failing any, options containing it.
    /// Case and spaces are ignored either way.
    pub fn matching(&self) -> Vec<(String, usize)> {
        let prefixed: Vec<(String, usize)> = self
            .options
            .iter()
            .filter(|(value, _)| Filter::loose_starts_with(value, &self.typed))
            .cloned()
            .collect();
        if !prefixed.is_empty() {
            return prefixed;
        }
        self.options
            .iter()
            .filter(|(value, _)| Filter::loose_contains(value, &self.typed))
            .cloned()
            .collect()
    }

    fn highlighted(&self) -> Option<(String, usize)> {
        self.matching().get(self.selected).cloned()
    }
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
    /// Shown columns in display order; header numbers are positions in this list.
    pub columns: Vec<Column>,
    pub paint: Vec<PaintRule>,
    pub views: ViewStore,
    /// Zero-based slot the current filter/sort came from.
    pub view: usize,
    pub filter_text: String,
    pub mode: Mode,
    pub input: String,
    pub pane: Pane,
    pub picker: Option<ValuePicker>,
    pub column_purpose: ColumnPurpose,
    pub sort: Option<Sort>,
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
            columns: Column::DEFAULT_SHOWN.to_vec(),
            paint: Vec::new(),
            views,
            view: 0,
            filter_text: String::new(),
            config,
            mode: Mode::Browse,
            input: String::new(),
            pane: Pane::None,
            picker: None,
            column_purpose: ColumnPurpose::Filter,
            sort: None,
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
            Some(saved)
                if saved.filter == self.filter_text
                    && saved.sort == self.sort
                    && saved.columns == self.columns
                    && saved.paint == self.paint =>
            {
                format!("v{}", self.view + 1)
            }
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
            self.filter_text,
            self.sort.map(|sort| sort.to_text()).unwrap_or_default(),
            self.pane
        )
    }

    /// `slot\tfilter\tsort\tselected`, enough to land where the user was after a self-restart.
    pub fn resume_state(&self) -> String {
        format!(
            "{}\t{}\t{}\t{}\t{}\t{}",
            self.view,
            self.filter_text,
            self.sort.map(|sort| sort.to_text()).unwrap_or_default(),
            self.selected,
            columns_text(&self.columns),
            rules_text(&self.paint)
        )
    }

    pub fn resume_from(&mut self, state: Option<&str>) {
        let Some(state) = state else {
            return;
        };
        let mut parts = state.split('\t');
        if let Some(slot) = parts.next().and_then(|n| n.parse().ok()) {
            self.view = slot;
        }
        let filter = parts.next().unwrap_or_default().to_string();
        self.sort = parts.next().and_then(Sort::parse);
        self.set_filter(filter);
        if let Some(selected) = parts.next().and_then(|n| n.parse().ok()) {
            self.select(selected);
        }
        if let Some(columns) = parts.next() {
            self.columns = parse_columns(columns);
        }
        if let Some(rules) = parts.next() {
            self.paint = parse_rules(rules);
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
        self.filter_text = self.filter_text.trim().to_string();
        let saved = SavedView {
            filter: self.filter_text.clone(),
            sort: self.sort,
            columns: self.columns.clone(),
            paint: self.paint.clone(),
        };
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
        self.picker = Some(ValuePicker {
            purpose: PickerPurpose::ChooseColumn(purpose),
            options: self.column_picker_options(),
            typed: String::new(),
            selected: 0,
        });
        self.mode = Mode::PickValue;
        self.status.clear();
    }

    fn open_column_purpose(&mut self, column: Column) {
        match self.column_purpose {
            ColumnPurpose::Filter => self.open_filter_picker(column),
            ColumnPurpose::Sort => self.open_sort_picker(column),
        }
    }

    fn open_paint_target_picker(&mut self) {
        let mut options: Vec<(String, usize)> = Vec::new();
        if let Some(task) = self.selected_task() {
            options.push((format!("row {}", task.id), 0));
        }
        if !self.filter_text.trim().is_empty() {
            options.push((format!("rows {}", self.filter_text.trim()), 0));
        }
        for column in &self.columns {
            options.push((format!("column {}", column.name()), 0));
        }
        self.picker = Some(ValuePicker {
            purpose: PickerPurpose::PaintTarget,
            options,
            typed: String::new(),
            selected: 0,
        });
        self.mode = Mode::PickValue;
        self.telemetry.record("action", "paint");
    }

    fn open_paint_color_picker(&mut self, target: PaintTarget) {
        let mut options: Vec<(String, usize)> = NAMED_COLORS
            .iter()
            .map(|name| (name.to_string(), 0))
            .collect();
        options.push(("none".to_string(), 0));
        self.picker = Some(ValuePicker {
            purpose: PickerPurpose::PaintColor(target),
            options,
            typed: String::new(),
            selected: 0,
        });
        self.mode = Mode::PickValue;
    }

    fn paint_target_from(&self, option: &str) -> Option<PaintTarget> {
        let (kind, rest) = option.split_once(' ')?;
        match kind {
            "row" => Some(PaintTarget::Rows(format!("id:{rest}"))),
            "rows" => Some(PaintTarget::Rows(rest.to_string())),
            "column" => Column::parse(rest).map(PaintTarget::Column),
            _ => None,
        }
    }

    fn apply_paint(&mut self, target: PaintTarget, color: &str) {
        self.paint = paint::with_rule(&self.paint, target.clone(), color);
        self.status = match color {
            "none" => "paint cleared".to_string(),
            _ => format!(
                "painted {}",
                PaintRule {
                    target,
                    color: color.to_string()
                }
                .to_text()
            ),
        };
        self.telemetry
            .record("action", format!("paint_apply {color}"));
    }

    fn open_columns_picker(&mut self) {
        self.picker = Some(ValuePicker {
            purpose: PickerPurpose::Columns,
            options: self.column_picker_options(),
            typed: String::new(),
            selected: 0,
        });
        self.mode = Mode::PickValue;
        self.telemetry.record("action", "columns");
    }

    /// Shown columns first in display order, then the hidden ones.
    fn column_picker_options(&self) -> Vec<(String, usize)> {
        self.columns
            .iter()
            .chain(
                Column::ALL
                    .iter()
                    .filter(|column| !self.columns.contains(column)),
            )
            .map(|column| (column.name().to_string(), 0))
            .collect()
    }

    fn toggle_column(&mut self, column: Column) {
        match self.columns.iter().position(|shown| *shown == column) {
            Some(index) if self.columns.len() > 1 => {
                self.columns.remove(index);
            }
            Some(_) => self.status = "at least one column must stay".to_string(),
            None => self.columns.push(column),
        }
        self.telemetry
            .record("action", format!("column_toggle {}", column.name()));
    }

    fn move_column(&mut self, column: Column, delta: isize) {
        let Some(index) = self.columns.iter().position(|shown| *shown == column) else {
            return;
        };
        let target = index as isize + delta;
        if target < 0 || target >= self.columns.len() as isize {
            return;
        }
        self.columns.swap(index, target as usize);
        self.telemetry
            .record("action", format!("column_move {} {delta}", column.name()));
    }

    fn open_filter_picker(&mut self, column: Column) {
        match column.filter_field() {
            Some(field) => {
                let options = tasks::field_values(&self.tasks, field);
                if options.is_empty() {
                    self.status = format!("no {} values to pick from", field.keyword());
                    return;
                }
                self.picker = Some(ValuePicker {
                    purpose: PickerPurpose::Filter(field),
                    options,
                    typed: String::new(),
                    selected: 0,
                });
                self.mode = Mode::PickValue;
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
        let mut options: Vec<(String, usize)> = sort::orders_for(column)
            .into_iter()
            .map(|order| (order.label(column), 0))
            .collect();
        options.push(("none".to_string(), 0));
        self.picker = Some(ValuePicker {
            purpose: PickerPurpose::Sort(column),
            options,
            typed: String::new(),
            selected: 0,
        });
        self.mode = Mode::PickValue;
        self.telemetry
            .record("action", format!("sort_column {}", column.header()));
    }

    fn handle_pick_value_key(&mut self, event: KeyEvent) {
        let Some(picker) = self.picker.as_mut() else {
            self.mode = Mode::Browse;
            return;
        };
        let last = picker.matching().len().saturating_sub(1);
        match event.code {
            KeyCode::Esc => {
                self.picker = None;
                self.mode = Mode::Browse;
            }
            KeyCode::Down => picker.selected = (picker.selected + 1).min(last),
            KeyCode::Up => picker.selected = picker.selected.saturating_sub(1),
            KeyCode::Char('j') if picker.typed.is_empty() => {
                picker.selected = (picker.selected + 1).min(last)
            }
            KeyCode::Char('k') if picker.typed.is_empty() => {
                picker.selected = picker.selected.saturating_sub(1)
            }
            KeyCode::Char(digit) if digit.is_ascii_digit() && picker.typed.is_empty() => {
                let index = digit.to_digit(10).unwrap_or(0) as usize;
                if (1..=picker.options.len()).contains(&index) {
                    picker.selected = index - 1;
                    self.apply_picked_value();
                }
            }
            KeyCode::Enter => self.apply_picked_value(),
            KeyCode::Char(' ') => self.toggle_picked_value(),
            KeyCode::Char(direction @ ('J' | 'K')) if picker.purpose == PickerPurpose::Columns => {
                if let Some((name, _)) = picker.highlighted() {
                    let delta = if direction == 'J' { 1 } else { -1 };
                    let moved_to = picker.selected as isize + delta;
                    if let Some(column) = Column::parse(&name) {
                        self.move_column(column, delta);
                    }
                    if let Some(picker) = self.picker.as_mut() {
                        picker.options = self
                            .columns
                            .iter()
                            .chain(
                                Column::ALL
                                    .iter()
                                    .filter(|column| !self.columns.contains(column)),
                            )
                            .map(|column| (column.name().to_string(), 0))
                            .collect();
                        picker.selected =
                            moved_to.clamp(0, picker.options.len() as isize - 1) as usize;
                    }
                }
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
        let Some((value, _)) = picker.highlighted() else {
            return;
        };
        match picker.purpose {
            PickerPurpose::Filter(field) => self.toggle_filter_value(field, &value),
            PickerPurpose::Columns => {
                if let Some(column) = Column::parse(&value) {
                    self.toggle_column(column);
                }
                let options = self.column_picker_options();
                if let Some(picker) = self.picker.as_mut() {
                    picker.options = options;
                }
            }
            PickerPurpose::Sort(_)
            | PickerPurpose::ChooseColumn(_)
            | PickerPurpose::PaintTarget
            | PickerPurpose::PaintColor(_) => {}
        }
    }

    fn toggle_filter_value(&mut self, field: FilterField, value: &str) {
        let Some(picker) = self.picker.as_ref() else {
            return;
        };
        let all: Vec<String> = picker.options.iter().map(|(v, _)| v.clone()).collect();
        let mut shown: Vec<String> = all
            .iter()
            .filter(|candidate| Filter::field_allows(&self.filter_text, field, candidate))
            .cloned()
            .collect();
        match shown.iter().position(|candidate| candidate == value) {
            Some(index) => {
                shown.remove(index);
            }
            None => shown.push(value.to_string()),
        }
        let text = Filter::with_shown(&self.filter_text, field, &all, &shown);
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
        let Some((value, _)) = picker.highlighted().or_else(|| {
            let typed = picker.typed.trim().to_string();
            match picker.purpose {
                PickerPurpose::PaintColor(_) if ratatui::style::Color::from_str(&typed).is_ok() => {
                    Some((typed, 0))
                }
                _ => None,
            }
        }) else {
            self.status = format!("nothing matches '{}'", picker.typed);
            return;
        };
        match picker.purpose {
            PickerPurpose::Filter(field) => {
                let text = Filter::with_only(&self.filter_text, field, &value);
                self.set_filter(text);
                self.telemetry
                    .record("action", format!("filter_pick {}:{value}", field.keyword()));
            }
            PickerPurpose::Sort(column) => {
                self.sort = sort::orders_for(column)
                    .into_iter()
                    .find(|order| order.label(column) == value)
                    .map(|order| Sort { column, order });
                self.refilter();
                self.telemetry
                    .record("action", format!("sort_pick {}:{value}", column.header()));
            }
            PickerPurpose::ChooseColumn(_) => {
                if let Some(column) = Column::parse(&value) {
                    self.open_column_purpose(column);
                }
            }
            PickerPurpose::Columns => {
                if let Some(column) = Column::parse(&value) {
                    self.toggle_column(column);
                }
            }
            PickerPurpose::PaintTarget => {
                if let Some(target) = self.paint_target_from(&value) {
                    self.open_paint_color_picker(target);
                }
            }
            PickerPurpose::PaintColor(target) => self.apply_paint(target, &value),
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
                    .record("action", format!("filter_apply {}", self.filter_text));
            }
            KeyCode::Backspace => {
                self.filter_text.pop();
                self.refilter();
            }
            KeyCode::Char(c) => {
                self.filter_text.push(c);
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
                } else if !self.filter_text.is_empty() {
                    self.set_filter(String::new());
                }
                self.status.clear();
            }
            Action::Filter => {
                self.mode = Mode::Filter;
                self.status.clear();
                if !self.filter_text.is_empty() && !self.filter_text.ends_with(' ') {
                    self.filter_text.push(' ');
                }
            }
            Action::FilterColumn => self.open_column_chooser(ColumnPurpose::Filter),
            Action::SortColumn => self.open_column_chooser(ColumnPurpose::Sort),
            Action::Columns => self.open_columns_picker(),
            Action::Paint => self.open_paint_target_picker(),
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
        self.sort = saved.sort;
        self.columns = saved.columns;
        self.paint = saved.paint;
        self.set_filter(saved.filter);
    }

    fn set_filter(&mut self, text: String) {
        self.filter_text = text;
        self.refilter();
    }

    fn refilter(&mut self) {
        let filter = Filter::parse(&self.filter_text);
        self.visible = (0..self.tasks.len())
            .filter(|&index| filter.matches(&self.tasks[index]))
            .collect();
        if let Some(sort) = self.sort {
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
