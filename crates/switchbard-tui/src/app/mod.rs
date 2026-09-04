//! Application state and the single place key events turn into state changes.
//! Submodules extend `App` by concept: `pickers`, `paint_flow`, `slots`.

mod paint_flow;
mod pickers;
mod slots;

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind};
use switchbard_core::{BacklogTask, GoalDef, WorkSession};

use crate::ball::Ball;
use crate::columns::Column;
use crate::config::{self, Action, Config, KeyChord};
use crate::group::{self, Grouping, Row};
use crate::paint;
use crate::picker::{ColumnPurpose, Payload, PickOption, PickerPurpose, ValuePicker};
use crate::report::{self, ReportContext, ReportKind};
use crate::settings::{Scope as SettingsScope, SettingsStore};
use crate::sort;
use crate::tasks::{self, Filter, GoalSummary, ProjectSummary};
use crate::telemetry::Telemetry;
use crate::views::{ViewState, ViewStore};

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
    /// After `t`: 1-5 ranks the selected task, `d` drops it, `p` pins/unpins the Top 5.
    RankChord,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pane {
    None,
    Detail,
    Help,
}

/// Where the app reads and writes outside the repo. Every field is optional:
/// `None` means that file is not consulted (tests, or no home directory).
#[derive(Debug, Clone, Default)]
pub struct AppPaths {
    pub config: Option<PathBuf>,
    pub global_views: Option<PathBuf>,
    pub repo_views: Option<PathBuf>,
    pub global_settings: Option<PathBuf>,
    pub repo_settings: Option<PathBuf>,
    /// The live-work session store (`switchbard_core::default_work_dir`).
    pub work_dir: Option<PathBuf>,
}

pub struct App {
    pub repo_root: PathBuf,
    pub config: Config,
    config_path: Option<PathBuf>,
    pub settings: SettingsStore,
    config_seen: Option<SystemTime>,
    tasks_seen: Option<SystemTime>,
    tasks: Vec<BacklogTask>,
    /// Project headings' facts, by stack rank; refreshed with the tasks.
    pub projects: Vec<ProjectSummary>,
    /// The repo's goal definitions; the goal column derives membership from them.
    pub goals: Vec<GoalDef>,
    /// Goal headings' facts for the current week; refreshed with the tasks.
    pub goal_summaries: Vec<GoalSummary>,
    /// The Top 5 (expedite lane) ids in order; the queue.
    pub top: Vec<String>,
    /// Live agent sessions holding tasks in this repo; refreshed every tick.
    pub work: Vec<WorkSession>,
    work_dir: Option<PathBuf>,
    /// When the app opened: the working-row blink counts from here.
    opened: Instant,
    /// Filtered and sorted task indices, the truth grouping projects from.
    pub visible: Vec<usize>,
    /// What the table shows: tasks, with a heading before each section when grouped.
    pub rows: Vec<Row>,
    /// Index into `rows`; never rests on a heading while a task row exists.
    pub selected: usize,
    /// First row on screen; the renderer keeps `selected` inside the window.
    pub scroll: usize,
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
    pub fn open(repo_root: &Path, paths: AppPaths, telemetry: Telemetry) -> App {
        let AppPaths {
            config: config_path,
            global_views,
            repo_views,
            global_settings,
            repo_settings,
            work_dir,
        } = paths;
        let config = config::load(config_path.as_deref());
        let (settings, settings_warnings) = SettingsStore::load(global_settings, repo_settings);
        let (views, view_warnings) = ViewStore::load(global_views, repo_views);
        let mut app = App {
            repo_root: repo_root.to_path_buf(),
            config_seen: config_path.as_deref().and_then(config::modified_at),
            config_path,
            settings,
            tasks_seen: None,
            tasks: Vec::new(),
            projects: Vec::new(),
            goals: Vec::new(),
            goal_summaries: Vec::new(),
            top: Vec::new(),
            work: Vec::new(),
            work_dir,
            opened: Instant::now(),
            visible: Vec::new(),
            rows: Vec::new(),
            selected: 0,
            scroll: 0,
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
        app.reload_work();
        app.switch_view(0);
        app.report_config_warnings();
        if let Some(warning) = view_warnings.first() {
            app.fail(format!("views: {warning}"));
        }
        if let Some(warning) = settings_warnings.first() {
            app.fail(format!("settings: {warning}"));
        }
        app
    }

    /// The task on a row; `None` for a heading.
    pub fn task(&self, row: usize) -> Option<&BacklogTask> {
        match self.rows.get(row)? {
            Row::Task(index) => Some(&self.tasks[*index]),
            Row::Heading { .. } => None,
        }
    }

    /// Whether more than one project exists, so grouping is worth pointing at.
    pub fn grouping_is_useful(&self) -> bool {
        self.projects.len() > 1
    }

    /// 1-based place in the Top 5, if the task is in it.
    pub fn rank_of(&self, task: &BacklogTask) -> Option<usize> {
        self.top.iter().position(|id| *id == task.id).map(|p| p + 1)
    }

    /// A cell's text: what the column says, plus what only the app knows (rank).
    pub fn cell(&self, column: Column, task: &BacklogTask) -> String {
        match column {
            Column::Rank => self
                .rank_of(task)
                .map(|n| n.to_string())
                .unwrap_or_default(),
            Column::Work => "●".repeat(self.working(task).len().min(3)),
            other => other.display_text(task, self.state.abbreviated.contains(&other), &self.goals),
        }
    }

    /// The initiative names behind the grouped projects, for the title bar.
    pub fn initiatives(&self) -> Vec<String> {
        group::initiatives(&self.projects)
    }

    pub fn selected_task(&self) -> Option<&BacklogTask> {
        self.task(self.selected)
    }

    pub fn tasks(&self) -> &[BacklogTask] {
        &self.tasks
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
        self.reload_work();
    }

    /// Re-read the live session records: a handful of small files, and the
    /// read is what notices a session's process has gone.
    fn reload_work(&mut self) {
        let Some(dir) = self.work_dir.as_deref() else {
            return;
        };
        match switchbard_core::list_work_sessions(dir, &self.repo_root) {
            Ok(sessions) => self.work = sessions,
            Err(error) => self.fail(format!("work sessions: {error}")),
        }
    }

    /// The live sessions working `task`, oldest first.
    pub fn working(&self, task: &BacklogTask) -> Vec<&WorkSession> {
        switchbard_core::sessions_working(&self.work, &task.id)
    }

    /// How many live sessions hold anything here: the title-bar count.
    pub fn working_sessions(&self) -> usize {
        self.work
            .iter()
            .filter(|session| !session.abandoned && !session.claims.is_empty())
            .count()
    }

    /// How bright a working row is this frame, 0 to 1: a sine over the
    /// period, peaking at its start, pushed through a `tanh` limiter so it
    /// lingers at full brightness and at dark and fades between them
    /// (`work.flatten`; 0 is the bare sine). Always 1 when the period is 0.
    pub fn work_glow(&self) -> f64 {
        let period = self.config.work_period_ms;
        if period == 0 {
            return 1.0;
        }
        let phase = (self.opened.elapsed().as_millis() % u128::from(period)) as f64 / period as f64;
        let wave = (std::f64::consts::TAU * phase).cos();
        let flatten = self.config.work_flatten;
        let clipped = if flatten <= 0.0 {
            wave
        } else {
            (flatten * wave).tanh() / flatten.tanh()
        };
        (1.0 + clipped) / 2.0
    }

    /// Time for the next redraw so a working row keeps pulsing while idle:
    /// one frame of the fade.
    pub fn next_blink(&self) -> Option<Duration> {
        let period = self.config.work_period_ms;
        if period == 0 || self.working_sessions() == 0 {
            return None;
        }
        Some(Duration::from_millis(
            (period / self.config.work_frames).max(16),
        ))
    }
    /// `w`: the owner passes the selected task; every session's claim on it ends.
    fn pass_work(&mut self) {
        let Some(task) = self.selected_task() else {
            self.status = "no task selected".to_string();
            return;
        };
        let id = task.id.clone();
        let Some(dir) = self.work_dir.clone() else {
            self.fail("no work store: no home directory".to_string());
            return;
        };
        match switchbard_core::pass_work(&dir, &self.repo_root, &id) {
            Ok(released) if released.is_empty() => {
                self.status = format!("{id}: no session is working it");
            }
            Ok(released) => {
                let _ = switchbard_core::set_backlog_ball(&self.repo_root, &id, None);
                self.status = format!(
                    "{id}: passed · released from {}",
                    released
                        .iter()
                        .map(WorkSession::short_id)
                        .collect::<Vec<_>>()
                        .join(", ")
                );
                self.telemetry
                    .record("action", format!("pass {}", released.len()));
                self.reload_work();
                self.reload_tasks();
            }
            Err(error) => self.fail(format!("{id}: {error}")),
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
            Mode::RankChord => self.handle_rank_chord_key(event),
        }
    }

    /// After `t`: digits accumulate in `input` (two digits once the list is long
    /// enough that a second could follow), Enter commits, `t` appends, `d` drops,
    /// `p` pins or unpins the section, `g` opens the goal panel.
    fn handle_rank_chord_key(&mut self, event: KeyEvent) {
        match event.code {
            KeyCode::Char(digit) if digit.is_ascii_digit() => {
                self.input.push(digit);
                let place: usize = self.input.parse().unwrap_or(0);
                let room = self.top.len() + 1;
                if place == 0 {
                    self.input.clear();
                    self.status = "rank: 1 is the top".to_string();
                    return;
                }
                if place * 10 <= room {
                    self.status = format!("rank: {place}▏ (another digit, or enter)");
                    return;
                }
                self.input.clear();
                self.mode = Mode::Browse;
                self.set_rank(place.min(room));
            }
            KeyCode::Enter if !self.input.is_empty() => {
                let place: usize = self.input.parse().unwrap_or(1);
                self.input.clear();
                self.mode = Mode::Browse;
                self.set_rank(place.max(1).min(self.top.len() + 1));
            }
            KeyCode::Char('t') => {
                self.input.clear();
                self.mode = Mode::Browse;
                self.set_rank(self.top.len() + 1);
            }
            KeyCode::Char('d') | KeyCode::Delete | KeyCode::Backspace => {
                self.input.clear();
                self.mode = Mode::Browse;
                self.drop_rank()
            }
            KeyCode::Char('g') => {
                self.input.clear();
                self.mode = Mode::Browse;
                self.open_goal_picker();
            }
            KeyCode::Char('p') => {
                self.input.clear();
                self.mode = Mode::Browse;
                self.state.pin_top = !self.state.pin_top;
                self.refilter();
                self.status = if self.state.pin_top {
                    "top list pinned first".to_string()
                } else {
                    "top list unpinned: ranked tasks sit in their sections".to_string()
                };
                self.telemetry
                    .record("action", format!("pin_top {}", self.state.pin_top));
            }
            KeyCode::Esc => {
                self.input.clear();
                self.mode = Mode::Browse;
                self.status.clear();
            }
            other => {
                self.input.clear();
                self.mode = Mode::Browse;
                self.status = format!("{other:?} is not a rank; digits, t, d, or p");
            }
        }
    }

    /// `t<n>`: the selected task takes place `n` in the top list; the rest shift down.
    fn set_rank(&mut self, place: usize) {
        let Some(task) = self.selected_task() else {
            self.status = "no task selected".to_string();
            return;
        };
        let id = task.id.clone();
        if let Err(error) = switchbard_core::expedite_task_at(&self.repo_root, &id, place) {
            self.fail(format!("{id}: {error}"));
            return;
        }
        self.reload_tasks();
        if !self.state.columns.contains(&Column::Rank) {
            self.state.columns.push(Column::Rank);
            self.refilter();
        }
        self.select_task(&id);
        self.status = format!("{id} is #{place} of {}", self.top.len());
        self.telemetry
            .record("action", format!("rank {place} {id}"));
    }

    fn drop_rank(&mut self) {
        let Some(task) = self.selected_task() else {
            self.status = "no task selected".to_string();
            return;
        };
        let id = task.id.clone();
        match switchbard_core::unexpedite_task(&self.repo_root, &id) {
            Ok(outcome) if outcome.changed() => {
                self.reload_tasks();
                self.select_task(&id);
                self.status = format!("{id} left the top list");
                self.telemetry.record("action", format!("unrank {id}"));
            }
            Ok(_) => self.status = format!("{id} was not in the top list"),
            Err(error) => self.fail(format!("{id}: {error}")),
        }
    }

    /// Put the cursor on `id` wherever the projection placed it.
    fn select_task(&mut self, id: &str) {
        if let Some(row) = self
            .rows
            .iter()
            .position(|row| matches!(row, Row::Task(index) if self.tasks[*index].id == id))
        {
            self.select(row);
        }
    }

    /// `b`: nobody → me → agent → nobody on the selected task, written as a label.
    fn pass_ball(&mut self) {
        let Some(task) = self.selected_task() else {
            self.status = "no task selected".to_string();
            return;
        };
        let (id, current) = (task.id.clone(), Ball::of(task));
        let next = Ball::next(current);
        match switchbard_core::set_backlog_ball(&self.repo_root, &id, next) {
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

    /// `o`: what to organize the list by. Project, goal, and the two nestings
    /// lead; the current choice is ✓, `x` flattens; picking the current
    /// choice flattens too.
    pub(super) fn open_organize_picker(&mut self) {
        let leading = [
            Grouping::by(Column::Project),
            Grouping::by(Column::Goal),
            Grouping::nested(Column::Project, Column::Goal),
            Grouping::nested(Column::Goal, Column::Project),
        ];
        let rest: Vec<Grouping> = Column::groupable_columns()
            .into_iter()
            .map(Grouping::by)
            .filter(|grouping| !leading.contains(grouping))
            .collect();
        let mut options: Vec<PickOption> = leading
            .into_iter()
            .chain(rest)
            .map(|grouping| {
                let mark = if self.state.group == grouping {
                    "✓"
                } else {
                    " "
                };
                PickOption {
                    label: format!("{mark}{}", grouping.name()),
                    count: 0,
                    key: None,
                    payload: Payload::Grouping(grouping),
                }
            })
            .collect();
        options.push(PickOption::keyed(
            'x',
            " off",
            Payload::Grouping(Grouping::flat()),
        ));
        self.open_picker(PickerPurpose::Organize, options);
        self.status.clear();
        self.telemetry
            .record("action", "organize_picker".to_string());
    }

    /// `tg`: the repo's goals, ✓ where the selected task is attached. Picking a
    /// row attaches or detaches through the core write layer and keeps the
    /// panel open, like `,`.
    pub(super) fn open_goal_picker(&mut self) {
        let Some(task) = self.selected_task() else {
            self.status = "no task selected".to_string();
            return;
        };
        if self.goals.is_empty() {
            self.status = "no goals in this repo · sb goal create makes one".to_string();
            return;
        }
        let id = task.id.clone();
        let options = self
            .goals
            .iter()
            .map(|goal| {
                let attached = goal
                    .inputs
                    .tasks
                    .iter()
                    .any(|t| t.eq_ignore_ascii_case(&id));
                let implied = !attached && goal.counts_task(task);
                let mark = if attached {
                    "✓"
                } else if implied {
                    "·"
                } else {
                    " "
                };
                PickOption {
                    label: format!("{mark}{}", goal.name),
                    count: 0,
                    key: None,
                    payload: Payload::Text(goal.name.clone()),
                }
            })
            .collect();
        self.open_picker(PickerPurpose::Goals(id), options);
        self.status = "✓ attached · · in scope already · enter toggles".to_string();
        self.telemetry.record("action", "goal_picker".to_string());
    }

    /// `:goal <name>` or a pick in the `tg` panel: attach the selected task to the
    /// goal, or detach it when already attached.
    pub(super) fn toggle_goal_link(&mut self, name: &str) {
        let Some(task) = self.selected_task() else {
            self.status = "no task selected".to_string();
            return;
        };
        let id = task.id.clone();
        let Some(goal) = self.goals.iter().find(|goal| goal.name == name) else {
            let names: Vec<&str> = self.goals.iter().map(|goal| goal.name.as_str()).collect();
            self.fail(if names.is_empty() {
                "no goals in this repo · sb goal create makes one".to_string()
            } else {
                format!("goal: one of {}", names.join(", "))
            });
            return;
        };
        let attached = goal
            .inputs
            .tasks
            .iter()
            .any(|t| t.eq_ignore_ascii_case(&id));
        let ids = [id.clone()];
        let result = if attached {
            switchbard_core::detach_goal_inputs(&self.repo_root, name, &ids, &[])
                .map(|_| "detached from")
        } else {
            switchbard_core::attach_goal_inputs(&self.repo_root, name, &ids, &[])
                .map(|_| "attached to")
        };
        match result {
            Ok(verb) => {
                self.telemetry.record(
                    "action",
                    format!("goal_{} {name}", if attached { "detach" } else { "attach" }),
                );
                self.reload_tasks();
                let highlighted = self.picker.as_ref().map(|p| p.selected);
                if matches!(
                    self.picker.as_ref().map(|p| &p.purpose),
                    Some(PickerPurpose::Goals(_))
                ) {
                    self.open_goal_picker();
                    if let (Some(picker), Some(row)) = (self.picker.as_mut(), highlighted) {
                        picker.selected = row;
                    }
                }
                self.status = format!("{id} {verb} {name}");
            }
            Err(error) => self.fail(format!("{id}: {error}")),
        }
    }

    /// Commands that start with what has been typed so far, for the footer hint.
    pub fn command_completions(&self) -> Vec<String> {
        let typed = self.input.split_whitespace().next().unwrap_or("");
        if self.input.contains(' ') {
            return Vec::new();
        }
        let mut names: Vec<String> = ["bug", "idea", "group", "palette", "reload", "q"]
            .iter()
            .map(|name| name.to_string())
            .collect();
        names.retain(|name| name.starts_with(typed) && name != typed);
        names
    }

    fn handle_browse_key(&mut self, event: KeyEvent) {
        let chord = KeyChord::from_event(&event);
        if let (KeyCode::Char(digit), false) = (event.code, chord.ctrl) {
            if let Some(position) = digit.to_digit(10).filter(|n| *n > 0) {
                self.open_column_actions(position as usize);
                return;
            }
        }
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
            Action::Down => self.step(1),
            Action::Up => self.step(-1),
            Action::Top => self.select(0),
            Action::Bottom => self.select(usize::MAX),
            Action::PageDown => self.step(self.page_size as isize),
            Action::PageUp => self.step(-(self.page_size as isize)),
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
            Action::Pass => self.pass_work(),
            Action::Settings => self.open_settings(),
            Action::Rank => {
                self.mode = Mode::RankChord;
                self.input.clear();
                self.status = format!(
                    "task: a number ranks it (1 is top, {} last) · t appends · d drops · p {} · g goals",
                    self.top.len() + 1,
                    if self.state.pin_top { "unpins" } else { "pins" }
                );
            }
            Action::Group => self.open_organize_picker(),
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
            "palette" => self.choose_palette(rest.trim()),
            "group" => match Grouping::parse(rest) {
                Some(grouping) => self.set_group(grouping),
                None => self.fail(format!(
                    "group by one of {}, two of them as a,b, or off",
                    Column::groupable_columns()
                        .iter()
                        .map(|column| column.name())
                        .collect::<Vec<_>>()
                        .join(", ")
                )),
            },
            "goal" => self.toggle_goal_link(rest.trim()),
            "bug" => self.file_report(ReportKind::Bug, rest),
            "idea" => self.file_report(ReportKind::Idea, rest),
            "" => {}
            other => self.fail(format!("unknown command :{other}")),
        }
    }

    /// `:palette <name>`: use a preset for this session and re-color every auto-painted
    /// value that still wears a preset color, so the change shows at once.
    fn choose_palette(&mut self, name: &str) {
        let names: Vec<String> = self
            .config
            .palettes
            .iter()
            .map(|(name, _)| name.clone())
            .collect();
        let Some((_, colors)) = self.config.palettes.iter().find(|(known, _)| known == name) else {
            self.fail(format!("palette: one of {}", names.join(", ")));
            return;
        };
        let colors = colors.clone();
        let known: Vec<Vec<String>> = self
            .config
            .palettes
            .iter()
            .map(|(_, colors)| colors.clone())
            .collect();
        paint::recolor_from_palettes(&mut self.state.paint, &known, &colors);
        self.config.palette = colors;
        self.status = format!("palette {name} · keep it: palette = \"{name}\" in tui.lua");
        self.telemetry.record("action", format!("palette {name}"));
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
        let target = self
            .config
            .report_repo
            .clone()
            .unwrap_or_else(|| self.repo_root.clone());
        let elsewhere = target != self.repo_root;
        match report::file_report(&target, kind, context) {
            Ok(bare_id) if elsewhere => {
                let repo = target
                    .file_name()
                    .map(|name| name.to_string_lossy().to_string())
                    .unwrap_or_default();
                self.status = format!("filed {bare_id} in {repo}");
                self.telemetry
                    .record("report", format!("{kind:?} {bare_id} in {repo}"));
            }
            Ok(bare_id) => {
                self.reload_tasks();
                let filed = self
                    .tasks
                    .iter()
                    .position(|task| task.id.rsplit('-').next() == Some(bare_id.as_str()));
                let shown_id = filed
                    .map(|index| self.tasks[index].id.clone())
                    .unwrap_or(bare_id);
                if let Some(row) = filed
                    .and_then(|index| self.rows.iter().position(|row| *row == Row::Task(index)))
                {
                    self.select(row);
                }
                self.status = format!("filed {shown_id}");
                self.telemetry
                    .record("report", format!("{kind:?} {shown_id}"));
            }
            Err(error) => self.fail(error.to_string()),
        }
    }

    fn set_filter(&mut self, text: String) {
        self.state.filter = text;
        self.refilter();
    }

    fn refilter(&mut self) {
        let base = self.settings.effective().base_filter(&self.state.filter);
        let filter = Filter::parse(&format!("{base} {}", self.state.filter));
        self.visible = (0..self.tasks.len())
            .filter(|&index| filter.matches(&self.tasks[index], &self.goals))
            .collect();
        if let Some(sort) = self.state.sort {
            sort::apply(&self.tasks, &mut self.visible, sort, &self.top, &self.goals);
        }
        let pinned: &[String] = if self.state.pin_top { &self.top } else { &[] };
        let headings = group::Headings {
            projects: &self.projects,
            goals: &self.goals,
            goal_summaries: &self.goal_summaries,
        };
        self.rows = group::rows(
            &self.tasks,
            &self.visible,
            &self.state.group,
            &headings,
            pinned,
        );
        self.select(self.selected);
    }

    /// `o`, `:group`, or a column's menu: organize the list, or flatten it.
    pub(super) fn set_group(&mut self, grouping: Grouping) {
        let kept = self.selected_task().map(|task| task.id.clone());
        self.state.group = grouping;
        self.refilter();
        if let Some(id) = kept {
            if let Some(row) = self
                .rows
                .iter()
                .position(|row| matches!(row, Row::Task(index) if self.tasks[*index].id == id))
            {
                self.select(row);
            }
        }
        self.status = if self.state.group.is_flat() {
            "flat list".to_string()
        } else {
            format!("organized by {} · o changes it", self.state.group.name())
        };
        self.telemetry
            .record("action", format!("group {}", self.state.group.name()));
    }

    /// `,`: the standing preferences, one row per status that can be hidden.
    pub(super) fn open_settings(&mut self) {
        let options = tasks::field_values(&self.tasks, tasks::FilterField::Status, &self.goals)
            .into_iter()
            .map(|(status, count)| {
                let mark = if self.settings.effective().is_hidden(&status) {
                    "✓"
                } else {
                    " "
                };
                PickOption {
                    label: format!("{mark}hide {status}"),
                    count,
                    key: None,
                    payload: Payload::Text(status),
                }
            })
            .collect();
        self.open_picker(PickerPurpose::Settings, options);
        self.status = match self.settings.scope() {
            SettingsScope::Repo => "this repo's settings".to_string(),
            SettingsScope::Global => "settings shared by every repo".to_string(),
        };
        self.telemetry.record("action", "settings");
    }

    /// A settings row picked: flip it for this repo, write the file, keep the panel open.
    pub(super) fn toggle_setting(&mut self, status: &str) {
        if let Err(error) = self
            .settings
            .edit_repo(|settings| settings.toggle_hidden(status))
        {
            self.fail(error);
        }
        self.refilter();
        let highlighted = self.picker.as_ref().map(|p| p.selected).unwrap_or(0);
        self.open_settings();
        if let Some(picker) = self.picker.as_mut() {
            picker.selected = highlighted;
        }
        self.status = match self.settings.effective().label() {
            Some(label) => format!("{label} · this repo · g makes it every repo"),
            None => "nothing hidden · this repo".to_string(),
        };
        self.telemetry
            .record("action", format!("settings_hide {status}"));
    }

    /// `g` in the settings panel: this repo's settings become every repo's.
    pub(super) fn promote_settings(&mut self) {
        match self.settings.promote() {
            Ok(()) => {
                self.status = match self.settings.effective().label() {
                    Some(label) => format!("{label} · every repo"),
                    None => "nothing hidden · every repo".to_string(),
                };
                self.telemetry.record("action", "settings_promote");
            }
            Err(error) => self.fail(error),
        }
        let highlighted = self.picker.as_ref().map(|p| p.selected).unwrap_or(0);
        let status = std::mem::take(&mut self.status);
        self.open_settings();
        self.status = status;
        if let Some(picker) = self.picker.as_mut() {
            picker.selected = highlighted;
        }
    }

    /// Land on `row`, or the nearest task row after it (before it at the end).
    fn select(&mut self, row: usize) {
        let last = self.rows.len().saturating_sub(1);
        let row = row.min(last);
        let forward = (row..=last).find(|&r| self.task(r).is_some());
        let backward = (0..row).rev().find(|&r| self.task(r).is_some());
        self.selected = forward.or(backward).unwrap_or(0);
    }

    /// Move `delta` task rows, headings not counting.
    fn step(&mut self, delta: isize) {
        let mut row = self.selected;
        let mut remaining = delta.unsigned_abs();
        while remaining > 0 {
            let next = if delta > 0 {
                (row + 1..self.rows.len()).find(|&r| self.task(r).is_some())
            } else {
                (0..row).rev().find(|&r| self.task(r).is_some())
            };
            match next {
                Some(next) => row = next,
                None => break,
            }
            remaining -= 1;
        }
        self.selected = row;
    }

    fn reload_tasks(&mut self) {
        self.tasks_seen = config::modified_at(&self.repo_root.join("backlog/tasks"));
        match tasks::load(&self.repo_root) {
            Ok(backlog) => {
                self.tasks = backlog.tasks;
                self.projects = backlog.projects;
                self.goals = backlog.goals;
                self.goal_summaries = backlog.goal_summaries;
                self.top = backlog.top;
            }
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
