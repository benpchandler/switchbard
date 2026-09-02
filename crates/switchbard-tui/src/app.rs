//! Application state and the single place key events turn into state changes.

use std::path::{Path, PathBuf};
use std::time::{Instant, SystemTime};

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind};
use switchbard_core::BacklogTask;

use crate::config::{self, Action, Column, Config, KeyChord};
use crate::report::{self, ReportContext, ReportKind};
use crate::sort::{self, Sort};
use crate::tasks::{self, Filter, FilterField};
use crate::telemetry::Telemetry;
use crate::views::{self, SavedView};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Browse,
    Filter,
    Command,
    PickColumn,
    PickValue,
    /// After `v`: a digit opens that slot, `s` starts a save.
    ViewChord,
    /// After `v s`: a digit or `d` (slot 1) picks the slot to save into.
    ViewSaveSlot,
    /// Naming the view being saved; Enter writes it, Esc abandons it.
    ViewName,
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
    views_path: Option<PathBuf>,
    pub views: Vec<SavedView>,
    /// Zero-based slot the current filter/sort came from.
    pub view: usize,
    saving_into: usize,
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
        views_path: Option<PathBuf>,
        telemetry: Telemetry,
    ) -> App {
        let config = config::load(config_path.as_deref());
        let (views, views_warning) = views::load(views_path.as_deref());
        let mut app = App {
            repo_root: repo_root.to_path_buf(),
            config_seen: config_path.as_deref().and_then(config::modified_at),
            config_path,
            tasks_seen: None,
            tasks: Vec::new(),
            visible: Vec::new(),
            selected: 0,
            views_path,
            views,
            view: 0,
            saving_into: 0,
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
        if let Some(warning) = views_warning {
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

    /// The slot's name while filter and sort still match it; `custom` once edited.
    pub fn view_label(&self) -> String {
        match self.views.get(self.view) {
            Some(saved) if saved.filter == self.filter_text && saved.sort == self.sort => {
                format!("{} {}", self.view + 1, saved.name)
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
            "{}\t{}\t{}\t{}",
            self.view,
            self.filter_text,
            self.sort.map(|sort| sort.to_text()).unwrap_or_default(),
            self.selected
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
            Mode::PickColumn => self.handle_pick_column_key(event),
            Mode::PickValue => self.handle_pick_value_key(event),
            Mode::ViewChord => self.handle_view_chord_key(event),
            Mode::ViewSaveSlot => self.handle_view_save_slot_key(event),
            Mode::ViewName => self.handle_view_name_key(event),
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
            other => self.status = format!("v then a slot number or s to save, not {other:?}"),
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
        self.saving_into = slot - 1;
        self.input = self
            .views
            .get(self.saving_into)
            .map(|saved| saved.name.clone())
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| self.suggested_view_name());
        self.mode = Mode::ViewName;
        self.status = format!("name for slot {slot} (enter saves, esc abandons)");
    }

    fn suggested_view_name(&self) -> String {
        if self.filter_text.is_empty() {
            "all".to_string()
        } else {
            self.filter_text
                .split_whitespace()
                .map(|word| {
                    word.rsplit(':')
                        .next()
                        .unwrap_or(word)
                        .trim_start_matches('!')
                })
                .collect::<Vec<_>>()
                .join(" ")
        }
    }

    fn handle_view_name_key(&mut self, event: KeyEvent) {
        match event.code {
            KeyCode::Esc => {
                self.mode = Mode::Browse;
                self.input.clear();
                self.status = "view not saved".to_string();
            }
            KeyCode::Enter => {
                self.mode = Mode::Browse;
                let name = std::mem::take(&mut self.input).trim().to_string();
                self.save_view(name);
            }
            KeyCode::Backspace => {
                self.input.pop();
            }
            KeyCode::Char(c) => self.input.push(c),
            _ => {}
        }
    }

    fn save_view(&mut self, name: String) {
        let saved = SavedView {
            name: if name.is_empty() {
                self.suggested_view_name()
            } else {
                name
            },
            filter: self.filter_text.trim().to_string(),
            sort: self.sort,
        };
        let slot = self.saving_into;
        if slot < self.views.len() {
            self.views[slot] = saved;
        } else {
            self.views.push(saved);
        }
        self.view = slot;
        self.filter_text = self.filter_text.trim().to_string();
        if let Some(path) = self.views_path.as_deref() {
            if let Err(error) = views::save(path, &self.views) {
                self.fail(format!("could not write {}: {error}", path.display()));
                return;
            }
        }
        self.status = format!("saved slot {} · v{} opens it", slot + 1, slot + 1);
        self.telemetry
            .record("action", format!("view_save {}", slot + 1));
    }

    fn handle_pick_column_key(&mut self, event: KeyEvent) {
        self.mode = Mode::Browse;
        let KeyCode::Char(digit) = event.code else {
            return;
        };
        let Some(column) = digit
            .to_digit(10)
            .and_then(|n| self.config.columns.get(n.checked_sub(1)? as usize))
            .copied()
        else {
            self.status = format!(
                "'{digit}' is not a column; press 1-{} as numbered in the header",
                self.config.columns.len()
            );
            return;
        };
        match self.column_purpose {
            ColumnPurpose::Filter => self.open_filter_picker(column),
            ColumnPurpose::Sort => self.open_sort_picker(column),
        }
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
        let PickerPurpose::Filter(field) = picker.purpose else {
            return;
        };
        let all: Vec<String> = picker.options.iter().map(|(v, _)| v.clone()).collect();
        let mut shown: Vec<String> = all
            .iter()
            .filter(|candidate| Filter::field_allows(&self.filter_text, field, candidate))
            .cloned()
            .collect();
        match shown.iter().position(|candidate| *candidate == value) {
            Some(index) => {
                shown.remove(index);
            }
            None => shown.push(value.clone()),
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
        let Some((value, _)) = picker.highlighted() else {
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
            Action::FilterColumn => {
                self.mode = Mode::PickColumn;
                self.column_purpose = ColumnPurpose::Filter;
                self.status = "filter by column: press its number".to_string();
            }
            Action::SortColumn => {
                self.mode = Mode::PickColumn;
                self.column_purpose = ColumnPurpose::Sort;
                self.status = "sort by column: press its number".to_string();
            }
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
                    "view: 1-{} opens a slot · s saves the current one",
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
        let Some(saved) = self.views.get(slot).cloned() else {
            return;
        };
        self.view = slot;
        self.sort = saved.sort;
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
