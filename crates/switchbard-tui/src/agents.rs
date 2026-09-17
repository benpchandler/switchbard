//! The Agents page: the live agent sessions in this repo (TASK-225).
//!
//! One row per running `claude`/`codex` session whose working directory is
//! inside one of this repo's worktrees — the same scope every other page
//! has. The rows come from `switchbard_core::scan_agent_sessions` (the OS
//! scan joined with Claude Code's own session listing), attributed with
//! the repo's `git worktree list`, and titled by
//! `switchbard_core::entitle_sessions`. Nothing here is stored: a poll
//! replaces the previous poll, and a session whose process is gone
//! disappears on the next one.
//!
//! ## Polling off the UI thread
//!
//! The listing spawns a `claude` process (about 150 ms) so, like the Pull
//! Requests page, the poll runs on a named worker thread and the UI reads
//! whatever the last completed poll produced. [`REFRESH_SECONDS`] bounds
//! how often; the navigation badge needs the count on every page, so the
//! poll runs regardless of which page is in front.
//!
//! ## What "last" means here
//!
//! The listing reports a session's state, not when it entered it. The
//! `last` column is the time since the session's own status line last
//! reported (`switchbard_core::agent_status`, fed by `sb agent status`),
//! which moves on every turn and so reads as "how long has this sat". A
//! session with no status record falls back to how long *this page* has
//! seen it in its current state, and only once it has seen the state
//! change — a session already idle when `sbt` started shows nothing rather
//! than a fabricated small number. Model and context use come from the
//! same record and are blank without it.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::Frame;
use switchbard_core::{AgentActivity, AgentSession, AgentStatus, WorktreeRef};

use crate::app::{App, Pane};
use crate::config::Surface;

/// How often the page re-polls: one `claude agents --json` spawn and one
/// process-table read per interval, per `sbt`.
pub const REFRESH_SECONDS: u64 = 5;

/// What one poll produced, built off the UI thread.
#[derive(Debug)]
pub struct Observation {
    /// Sessions attributed to this repo, titled, in display order.
    pub sessions: Vec<AgentSession>,
    /// Why the Claude listing contributed nothing, when it did not.
    pub listing_error: Option<String>,
    /// Each session's last self-report, keyed by session id.
    pub statuses: HashMap<String, AgentStatus>,
    /// Unix seconds when the poll ran: what `statuses` ages are measured from.
    pub observed_unix: u64,
}

/// The page's state: the last poll, the cursor, and the worker.
#[derive(Default)]
pub struct Agents {
    pub rows: Vec<AgentSession>,
    pub listing_error: Option<String>,
    /// See [`Observation::statuses`].
    pub statuses: HashMap<String, AgentStatus>,
    observed_unix: u64,
    /// The poll itself failed (the process scan errored); the rows are
    /// whatever the last good poll left.
    pub error: Option<String>,
    pub selected: usize,
    pub scroll: usize,
    pub detail_scroll: u16,
    /// False stops the worker from being started: tests inject observations
    /// through [`Agents::accept`] and must not have them overwritten.
    pub live: bool,
    observed_at: Option<Instant>,
    completed_at: Option<Instant>,
    pending: Option<Receiver<Result<Observation, String>>>,
    /// Per session: the state last seen and, once a change has been
    /// observed, when — see the module doc.
    state_since: HashMap<String, (AgentActivity, Option<Instant>)>,
    /// First prompts, shared with the worker; a transcript is read once.
    prompt_cache: Arc<Mutex<HashMap<String, Option<String>>>>,
}

impl Agents {
    pub fn new() -> Self {
        Agents {
            live: true,
            ..Agents::default()
        }
    }

    /// Start a poll unless one is in flight. `held` maps a session id to
    /// the title of a task it holds, read from the live-work store by the
    /// caller, so the worker never touches that store itself.
    pub fn refresh(&mut self, repo_root: &Path, held: HashMap<String, String>) {
        if self.pending.is_some() || !self.live {
            return;
        }
        let (tx, rx) = mpsc::sync_channel(1);
        let root = repo_root.to_path_buf();
        let cache = Arc::clone(&self.prompt_cache);
        let claude_home = switchbard_core::default_claude_home();
        match std::thread::Builder::new()
            .name("sbt-agents-poll".into())
            .spawn(move || {
                // A closed receiver means the app quit; nothing left to publish.
                let _ = tx.send(observe(&root, held, claude_home, &cache));
            }) {
            Ok(_) => self.pending = Some(rx),
            Err(error) => {
                self.error = Some(format!("Could not start agent poll: {error}"));
                self.completed_at = Some(Instant::now());
            }
        }
    }

    /// Collect a finished poll and start the next one when due. Returns
    /// true when the rows changed.
    pub fn tick(&mut self, repo_root: &Path, held: HashMap<String, String>, now: Instant) -> bool {
        let mut changed = false;
        if let Some(rx) = &self.pending {
            match rx.try_recv() {
                Ok(Ok(observation)) => {
                    self.pending = None;
                    self.completed_at = Some(now);
                    changed = self.accept(observation, now);
                }
                Ok(Err(error)) => {
                    self.pending = None;
                    self.completed_at = Some(now);
                    self.error = Some(error);
                }
                Err(TryRecvError::Disconnected) => {
                    self.pending = None;
                    self.completed_at = Some(now);
                    self.error = Some("Agent poll worker stopped; reload to retry".into());
                }
                Err(TryRecvError::Empty) => {}
            }
        }
        let due = self.completed_at.is_none_or(|completed| {
            now.saturating_duration_since(completed) >= Duration::from_secs(REFRESH_SECONDS)
        });
        if due {
            self.refresh(repo_root, held);
        }
        changed
    }

    /// Take a poll's result as the page's truth. Public so a test can feed
    /// sessions without a `claude` on the machine; set [`Agents::live`]
    /// false first or the next poll replaces them.
    pub fn accept(&mut self, observation: Observation, now: Instant) -> bool {
        let previous_key = self.row().map(session_key);
        let mut sessions = observation.sessions;
        sort_for_display(&mut sessions);
        let changed = self.rows != sessions;
        self.error = None;
        self.listing_error = observation.listing_error;
        self.statuses = observation.statuses;
        self.observed_unix = observation.observed_unix;
        self.observed_at = Some(now);
        self.track_states(&sessions, now);
        self.rows = sessions;
        self.selected = previous_key
            .and_then(|key| self.rows.iter().position(|row| session_key(row) == key))
            .unwrap_or_else(|| self.selected.min(self.rows.len().saturating_sub(1)));
        changed
    }

    /// Ask for a poll now regardless of the interval.
    pub fn poll_now(&mut self, repo_root: &Path, held: HashMap<String, String>) {
        self.completed_at = None;
        self.refresh(repo_root, held);
    }

    fn track_states(&mut self, sessions: &[AgentSession], now: Instant) {
        let mut next = HashMap::with_capacity(sessions.len());
        for session in sessions {
            let key = session_key(session);
            let entry = match self.state_since.get(&key) {
                Some((seen, since)) if *seen == session.activity => (*seen, *since),
                Some(_) => (session.activity, Some(now)),
                None => (session.activity, None),
            };
            next.insert(key, entry);
        }
        self.state_since = next;
    }

    pub fn row(&self) -> Option<&AgentSession> {
        self.rows.get(self.selected)
    }

    /// The session's last self-report, when it has one.
    pub fn status_of(&self, session: &AgentSession) -> Option<&AgentStatus> {
        session
            .session_id
            .as_deref()
            .and_then(|id| self.statuses.get(id))
    }

    /// Time since the session last reported, else since this page saw its
    /// state change — see the module doc. `now` is the render instant;
    /// status ages advance from the poll's own clock by the same elapsed.
    pub fn last_activity(&self, session: &AgentSession, now: Instant) -> Option<Duration> {
        if let (Some(status), Some(observed)) = (self.status_of(session), self.observed_at) {
            let since_poll = now.saturating_duration_since(observed);
            return Some(status.age(self.observed_unix) + since_poll);
        }
        self.state_age(session, now)
    }

    /// How long the selected session has been in its state, when known.
    pub fn state_age(&self, session: &AgentSession, now: Instant) -> Option<Duration> {
        self.state_since
            .get(&session_key(session))
            .and_then(|(_, since)| since.map(|since| now.saturating_duration_since(since)))
    }

    /// Sessions waiting on a human: the navigation badge.
    pub fn idle_count(&self) -> usize {
        self.rows
            .iter()
            .filter(|row| row.activity == AgentActivity::Idle)
            .count()
    }

    /// Never observed, and not failed either: the badge's "…".
    pub fn loading(&self) -> bool {
        self.observed_at.is_none() && self.error.is_none()
    }

    pub fn observed_at(&self) -> Option<Instant> {
        self.observed_at
    }

    pub fn step(&mut self, delta: isize) {
        let previous = self.selected;
        self.selected = self
            .selected
            .saturating_add_signed(delta)
            .min(self.rows.len().saturating_sub(1));
        if self.selected != previous {
            self.detail_scroll = 0;
        }
    }
}

/// The identity a cursor and a state-age follow across polls.
fn session_key(session: &AgentSession) -> String {
    session
        .session_id
        .clone()
        .unwrap_or_else(|| format!("pid:{}", session.pid))
}

/// The worker's whole job: scan, attribute to this repo, drop everything
/// else, and title. Ordering is [`Agents::accept`]'s, so injected and
/// polled observations display alike.
fn observe(
    repo_root: &Path,
    held: HashMap<String, String>,
    claude_home: Option<PathBuf>,
    cache: &Arc<Mutex<HashMap<String, Option<String>>>>,
) -> Result<Observation, String> {
    let scan = switchbard_core::scan_agent_sessions().map_err(|e| e.to_string())?;
    let worktrees = repo_worktrees(repo_root);
    let mut sessions = attribute_to_repo(&scan.rows, &worktrees);
    let observed_unix = switchbard_core::dispatch_inspect::now_unix();
    let statuses = match switchbard_core::default_agent_status_dir() {
        Some(dir) => {
            switchbard_core::load_agent_statuses(&dir, observed_unix).map_err(|e| e.to_string())?
        }
        None => HashMap::new(),
    };
    {
        let mut cache = cache
            .lock()
            .map_err(|_| "prompt cache poisoned".to_string())?;
        switchbard_core::entitle_sessions(
            &mut sessions,
            claude_home.as_deref(),
            |session| {
                session
                    .session_id
                    .as_deref()
                    .and_then(|id| statuses.get(id))
                    .and_then(|status| status.session_name.clone())
            },
            |session| {
                session
                    .session_id
                    .as_deref()
                    .and_then(|id| held.get(id).cloned())
            },
            &mut cache,
        );
    }
    Ok(Observation {
        sessions,
        listing_error: scan.listing_error,
        statuses,
        observed_unix,
    })
}

/// Every scanned process whose cwd falls inside one of `worktrees`; the
/// rest is another repo's business, or nobody's. Pure.
pub fn attribute_to_repo(
    rows: &[switchbard_core::AgentProcessRow],
    worktrees: &[WorktreeRef],
) -> Vec<AgentSession> {
    let mut sessions = switchbard_core::attribute_agent_sessions(rows, worktrees);
    sessions.retain(|session| session.worktree_path.is_some());
    sessions
}

/// Idle sessions first — they are waiting on the reader — then busy, then
/// those with no state; oldest first within a state, pid as the tiebreak.
pub fn sort_for_display(sessions: &mut [AgentSession]) {
    sessions.sort_by_key(|session| {
        (
            match session.activity {
                AgentActivity::Idle => 0,
                AgentActivity::Busy => 1,
                AgentActivity::Unknown => 2,
            },
            session.started_unix.unwrap_or(u64::MAX),
            session.pid,
        )
    });
}

/// This repo's worktrees as attribution targets. Outside a git repository
/// (a bare backlog directory) the root itself is the one worktree, so a
/// session started there still attributes.
fn repo_worktrees(repo_root: &Path) -> Vec<WorktreeRef> {
    let repo_name = repo_root
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut refs: Vec<WorktreeRef> = switchbard_core::enumerate_worktrees(repo_root)
        .unwrap_or_default()
        .into_iter()
        .filter(|entry| entry.path.exists())
        .map(|entry| WorktreeRef {
            repo_name: repo_name.clone(),
            path: entry.path,
            branch: entry.branch,
            head: entry.head,
        })
        .collect();
    if !refs.iter().any(|w| w.path == repo_root) {
        refs.push(WorktreeRef {
            repo_name,
            path: repo_root.to_path_buf(),
            branch: None,
            head: String::new(),
        });
    }
    refs
}

/// The state glyph next to its word: what the cell and the detail print.
pub fn state_label(activity: AgentActivity) -> String {
    let glyph = match activity {
        AgentActivity::Idle => "✳",
        AgentActivity::Busy => "◐",
        AgentActivity::Unknown => "?",
    };
    format!("{glyph} {}", activity.label())
}

fn state_surface(activity: AgentActivity) -> Surface {
    match activity {
        AgentActivity::Idle => Surface::Accent,
        AgentActivity::Busy => Surface::Working,
        AgentActivity::Unknown => Surface::Hint,
    }
}

/// `4s`, `12m`, `3h` — the compact age for a cell.
pub fn short_age(age: Duration) -> String {
    let secs = age.as_secs();
    if secs < 60 {
        format!("{secs}s")
    } else if secs < 3_600 {
        format!("{}m", secs / 60)
    } else if secs < 86_400 {
        format!("{}h", secs / 3_600)
    } else {
        format!("{}d", secs / 86_400)
    }
}

// ---------------------------------------------------------------- rendering

pub fn draw(frame: &mut Frame, app: &mut App, area: Rect) {
    if app.pane == Pane::Detail {
        let [left, right] = crate::detail_pane::split(area);
        draw_list(frame, app, left);
        detail(frame, app, right);
    } else {
        draw_list(frame, app, area);
    }
}

fn draw_list(frame: &mut Frame, app: &mut App, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Agents ")
        .border_style(app.config.theme.style(Surface::Border));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let [status, body] = Layout::vertical([Constraint::Length(2), Constraint::Min(0)]).areas(inner);
    frame.render_widget(
        Paragraph::new(observation(app, Instant::now()))
            .wrap(Wrap { trim: false })
            .style(app.config.theme.style(Surface::Hint)),
        status,
    );
    list(frame, app, body);
}

/// The two hint lines above the list: coverage and freshness, then any
/// degradation the reader should know about.
fn observation(app: &App, now: Instant) -> String {
    let agents = &app.agents;
    let first = match (agents.observed_at(), &agents.error) {
        (None, None) => "Polling agent sessions...".to_string(),
        (None, Some(error)) => format!("Unavailable: {error}"),
        (Some(observed), _) => {
            let idle = agents.idle_count();
            let busy = agents
                .rows
                .iter()
                .filter(|row| row.activity == AgentActivity::Busy)
                .count();
            format!(
                "{} in this repo · {idle} idle · {busy} busy · observed {} ago",
                match agents.rows.len() {
                    1 => "1 session".to_string(),
                    n => format!("{n} sessions"),
                },
                short_age(now.saturating_duration_since(observed))
            )
        }
    };
    let second = match (&agents.error, &agents.listing_error) {
        (Some(error), _) if agents.observed_at().is_some() => format!("Last poll failed: {error}"),
        (_, Some(error)) => format!("claude listing unavailable, states unknown: {error}"),
        _ => String::new(),
    };
    format!("{first}\n{second}")
}

struct Columns {
    labels: Vec<&'static str>,
    widths: Vec<Constraint>,
    /// Model and context use: dropped below the medium width.
    report: bool,
    worktree: bool,
    tasks: bool,
}

/// Below this width the page shows state, last and title only; between
/// it and [`WIDE_WIDTH`] model and context join; worktree and tasks need
/// the full width.
const NARROW_WIDTH: u16 = 64;
const WIDE_WIDTH: u16 = 96;

fn columns(app: &App, width: u16) -> Columns {
    let rows = &app.agents.rows;
    let narrow = width < NARROW_WIDTH;
    let wide = width >= WIDE_WIDTH;
    let branch_width = rows
        .iter()
        .map(|row| worktree_text(row).chars().count())
        .max()
        .unwrap_or(0)
        .clamp(8, 24) as u16;
    let tasks_width = rows
        .iter()
        .map(|row| tasks_text(app, row).chars().count())
        .max()
        .unwrap_or(0)
        .clamp(5, 18) as u16;
    let model_width = rows
        .iter()
        .filter_map(|row| app.agents.status_of(row))
        .map(|status| model_text(status).chars().count())
        .max()
        .unwrap_or(0)
        .clamp(5, 14) as u16;
    let mut labels = vec!["state", "last"];
    let mut widths = vec![Constraint::Length(9), Constraint::Length(4)];
    if !narrow {
        labels.push("model");
        widths.push(Constraint::Length(model_width));
        labels.push("ctx");
        widths.push(Constraint::Length(4));
    }
    if wide {
        labels.push("worktree");
        widths.push(Constraint::Length(branch_width));
    }
    labels.push("title");
    widths.push(Constraint::Min(8));
    if wide {
        labels.push("tasks");
        widths.push(Constraint::Length(tasks_width));
    }
    Columns {
        labels,
        widths,
        report: !narrow,
        worktree: wide,
        tasks: wide,
    }
}

fn model_text(status: &AgentStatus) -> String {
    status
        .model_name
        .clone()
        .or_else(|| status.model_id.clone())
        .unwrap_or_default()
}

fn context_text(status: &AgentStatus) -> String {
    status
        .context_used_percent
        .map(|percent| format!("{percent:.0}%"))
        .unwrap_or_default()
}

fn worktree_text(row: &AgentSession) -> String {
    row.worktree_branch
        .clone()
        .or_else(|| {
            row.worktree_path
                .as_ref()
                .and_then(|path| path.file_name())
                .map(|name| name.to_string_lossy().into_owned())
        })
        .unwrap_or_else(|| "-".to_string())
}

/// The task ids the session holds: the live-work store's claims, joined
/// on session id. Read from the app's own `work` snapshot so the cell, the
/// detail and the Tasks page's work column all answer from one read.
fn held_ids(app: &App, row: &AgentSession) -> Vec<String> {
    let Some(id) = row.session_id.as_deref() else {
        return Vec::new();
    };
    app.work
        .iter()
        .filter(|session| !session.abandoned && session.session_id == id)
        .flat_map(|session| session.claims.iter().map(|claim| claim.task_id.clone()))
        .collect()
}

fn tasks_text(app: &App, row: &AgentSession) -> String {
    match held_ids(app, row) {
        ids if ids.is_empty() => "-".to_string(),
        ids => ids.join(","),
    }
}

fn list(frame: &mut Frame, app: &mut App, area: Rect) {
    if area.height == 0 {
        return;
    }
    if app.agents.rows.is_empty() {
        let text = if app.agents.observed_at().is_some() {
            "No agent sessions are running in this repo."
        } else {
            ""
        };
        frame.render_widget(
            Paragraph::new(text).style(app.config.theme.style(Surface::Hint)),
            area,
        );
        return;
    }
    let now = Instant::now();
    let layout = columns(app, area.width);
    let viewport = crate::list_presentation::ListViewport::new(
        app.agents.scroll,
        app.agents.selected,
        app.agents.rows.len(),
        area.height.saturating_sub(1) as usize,
        false,
    );
    app.agents.scroll = viewport.scroll;
    let header = Rect { height: 1, ..area };
    let cells = crate::list_presentation::cells(header, &layout.widths);
    let labels: Vec<Line<'static>> = layout.labels.iter().map(|s| Line::from(*s)).collect();
    crate::list_presentation::header(
        frame,
        header,
        &cells,
        &labels,
        app.config.theme.style(Surface::Header),
    );
    for (offset, index) in (app.agents.scroll..app.agents.rows.len())
        .take(viewport.slots)
        .enumerate()
    {
        let rect = Rect {
            y: area.y + 1 + offset as u16,
            height: 1,
            ..area
        };
        draw_row(frame, app, index, rect, &layout, now);
    }
}

fn draw_row(
    frame: &mut Frame,
    app: &App,
    index: usize,
    rect: Rect,
    layout: &Columns,
    now: Instant,
) {
    let theme = &app.config.theme;
    let row = &app.agents.rows[index];
    let selected = index == app.agents.selected;
    let last = app
        .agents
        .last_activity(row, now)
        .map(short_age)
        .unwrap_or_default();
    let mut texts: Vec<(String, Surface)> = vec![
        (state_label(row.activity), state_surface(row.activity)),
        (last, Surface::Hint),
    ];
    if layout.report {
        let status = app.agents.status_of(row);
        texts.push((status.map(model_text).unwrap_or_default(), Surface::Hint));
        texts.push((status.map(context_text).unwrap_or_default(), Surface::Hint));
    }
    if layout.worktree {
        texts.push((worktree_text(row), Surface::Link));
    }
    texts.push((
        row.title
            .clone()
            .unwrap_or_else(|| switchbard_core::UNNAMED_SESSION_TITLE.to_string()),
        Surface::Text,
    ));
    if layout.tasks {
        texts.push((tasks_text(app, row), Surface::Label));
    }
    let base = theme.style(if selected {
        Surface::Selected
    } else {
        Surface::Text
    });
    frame.render_widget(Paragraph::new("").style(base), rect);
    let cells = crate::list_presentation::cells(rect, &layout.widths);
    for ((text, surface), cell) in texts.iter().zip(cells.iter()) {
        let mut style = theme.style(*surface);
        if selected {
            style = style.patch(theme.style(Surface::Selected));
        }
        frame.render_widget(Paragraph::new(text.as_str()).style(style), *cell);
    }
}

fn detail(frame: &mut Frame, app: &mut App, area: Rect) {
    let lines = detail_lines(app, Instant::now());
    app.agents.detail_scroll = crate::detail_pane::draw(
        frame,
        &app.config.theme,
        area,
        lines,
        app.agents.detail_scroll,
    );
}

fn detail_lines(app: &App, now: Instant) -> Vec<Line<'static>> {
    let Some(row) = app.agents.row() else {
        return vec![Line::from("nothing selected")];
    };
    let theme = &app.config.theme;
    let state = match app.agents.last_activity(row, now) {
        Some(age) => format!(
            "{} · last activity {} ago",
            state_label(row.activity),
            short_age(age)
        ),
        None => state_label(row.activity),
    };
    let mut lines = vec![
        crate::detail_pane::title(
            row.title
                .clone()
                .unwrap_or_else(|| switchbard_core::UNNAMED_SESSION_TITLE.to_string()),
        ),
        Line::from(Span::styled(
            state,
            theme.style(state_surface(row.activity)),
        )),
        crate::detail_pane::metadata(format!("{} · pid {}", row.kind.label(), row.pid), theme),
    ];
    if let Some(id) = &row.session_id {
        lines.push(crate::detail_pane::metadata(format!("session {id}"), theme));
    }
    if let Some(name) = row
        .name
        .as_deref()
        .filter(|name| Some(*name) != row.title.as_deref())
    {
        lines.push(crate::detail_pane::metadata(
            format!("listed as {name}"),
            theme,
        ));
    }
    if let Some(started) = row.started_unix {
        lines.push(crate::detail_pane::metadata(
            format!("started {}", switchbard_core::humanize_age(started)),
            theme,
        ));
    }
    if let Some(status) = app.agents.status_of(row) {
        let mut report = Vec::new();
        match (&status.model_name, &status.model_id) {
            (Some(name), Some(id)) => report.push(format!("{name} ({id})")),
            (Some(name), None) => report.push(name.clone()),
            (None, Some(id)) => report.push(id.clone()),
            (None, None) => {}
        }
        if let Some(percent) = status.context_used_percent {
            report.push(format!("{percent:.0}% context"));
        }
        if let Some(cost) = status.total_cost_usd {
            report.push(format!("${cost:.2}"));
        }
        if let Some(version) = &status.claude_version {
            report.push(format!("v{version}"));
        }
        if !report.is_empty() {
            lines.push(crate::detail_pane::metadata(report.join(" · "), theme));
        }
    } else {
        lines.push(crate::detail_pane::metadata(
            "no status report (status line not wired to sb agent status)".to_string(),
            theme,
        ));
    }
    lines.push(Line::from(""));
    lines.push(crate::detail_pane::section("worktree", theme));
    lines.push(Line::from(worktree_text(row)));
    if let Some(path) = &row.worktree_path {
        lines.push(Line::from(path.display().to_string()));
    }
    if let Some(cwd) = row
        .cwd
        .as_ref()
        .filter(|cwd| Some(*cwd) != row.worktree_path.as_ref())
    {
        lines.push(crate::detail_pane::metadata(
            format!("cwd {}", cwd.display()),
            theme,
        ));
    }
    lines.push(Line::from(""));
    lines.push(crate::detail_pane::section("holds", theme));
    let held = held_ids(app, row);
    if held.is_empty() {
        lines.push(crate::detail_pane::metadata(
            "no claimed tasks".to_string(),
            theme,
        ));
    }
    for id in held {
        let text = match app.tasks().iter().find(|task| task.id == id) {
            Some(task) => {
                let done = task
                    .acceptance_criteria
                    .iter()
                    .filter(|item| item.checked)
                    .count();
                format!(
                    "{id} · {} · {done}/{} AC",
                    task.title,
                    task.acceptance_criteria.len()
                )
            }
            None => id,
        };
        lines.push(Line::from(text));
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use switchbard_core::AgentProcessKind;

    fn session(pid: u32, activity: AgentActivity, started: Option<u64>) -> AgentSession {
        AgentSession {
            pid,
            process_identity: None,
            kind: AgentProcessKind::Claude,
            cwd: None,
            repo_name: None,
            worktree_path: None,
            worktree_branch: None,
            started_unix: started,
            pgid: None,
            session_id: Some(format!("sid-{pid}")),
            name: None,
            activity,
            title: None,
        }
    }

    #[test]
    fn display_order_is_idle_busy_unknown_then_oldest() {
        let mut rows = vec![
            session(3, AgentActivity::Unknown, Some(10)),
            session(2, AgentActivity::Busy, Some(5)),
            session(1, AgentActivity::Idle, Some(50)),
            session(4, AgentActivity::Idle, Some(20)),
            session(5, AgentActivity::Idle, None),
        ];
        sort_for_display(&mut rows);
        let pids: Vec<u32> = rows.iter().map(|r| r.pid).collect();
        assert_eq!(pids, vec![4, 1, 5, 2, 3]);
    }

    #[test]
    fn state_age_appears_only_after_an_observed_change_and_follows_the_session() {
        let mut agents = Agents::default();
        let t0 = Instant::now();
        agents.accept(
            Observation {
                sessions: vec![session(1, AgentActivity::Idle, None)],
                listing_error: None,
                statuses: HashMap::new(),
                observed_unix: 0,
            },
            t0,
        );
        assert_eq!(
            agents.state_age(&agents.rows[0], t0),
            None,
            "first sight: no age"
        );
        let t1 = t0 + Duration::from_secs(30);
        agents.accept(
            Observation {
                sessions: vec![session(1, AgentActivity::Busy, None)],
                listing_error: None,
                statuses: HashMap::new(),
                observed_unix: 0,
            },
            t1,
        );
        let t2 = t1 + Duration::from_secs(12);
        assert_eq!(
            agents.state_age(&agents.rows[0], t2),
            Some(Duration::from_secs(12))
        );
        agents.accept(
            Observation {
                sessions: vec![session(1, AgentActivity::Busy, None)],
                listing_error: None,
                statuses: HashMap::new(),
                observed_unix: 0,
            },
            t2,
        );
        assert_eq!(
            agents.state_age(&agents.rows[0], t2 + Duration::from_secs(1)),
            Some(Duration::from_secs(13)),
            "an unchanged state keeps its clock"
        );
        agents.accept(
            Observation {
                sessions: vec![],
                listing_error: None,
                statuses: HashMap::new(),
                observed_unix: 0,
            },
            t2,
        );
        assert!(agents.rows.is_empty());
        assert_eq!(agents.idle_count(), 0);
    }

    #[test]
    fn the_cursor_follows_its_session_across_a_poll_and_clamps_when_it_goes() {
        let mut agents = Agents::default();
        let now = Instant::now();
        let three = vec![
            session(1, AgentActivity::Idle, Some(1)),
            session(2, AgentActivity::Idle, Some(2)),
            session(3, AgentActivity::Idle, Some(3)),
        ];
        agents.accept(
            Observation {
                sessions: three.clone(),
                listing_error: None,
                statuses: HashMap::new(),
                observed_unix: 0,
            },
            now,
        );
        agents.step(2);
        assert_eq!(agents.row().unwrap().pid, 3);
        agents.accept(
            Observation {
                sessions: vec![three[2].clone(), three[0].clone()],
                listing_error: None,
                statuses: HashMap::new(),
                observed_unix: 0,
            },
            now,
        );
        assert_eq!(agents.row().unwrap().pid, 3, "same session, new position");
        agents.accept(
            Observation {
                sessions: vec![three[0].clone()],
                listing_error: None,
                statuses: HashMap::new(),
                observed_unix: 0,
            },
            now,
        );
        assert_eq!(agents.selected, 0, "clamped into the shorter list");
        agents.step(-5);
        assert_eq!(agents.selected, 0);
    }

    #[test]
    fn only_sessions_inside_this_repos_worktrees_survive_attribution() {
        let worktrees = vec![
            WorktreeRef {
                repo_name: "alpha".into(),
                path: PathBuf::from("/w/alpha"),
                branch: Some("main".into()),
                head: String::new(),
            },
            WorktreeRef {
                repo_name: "alpha".into(),
                path: PathBuf::from("/w/.worktrees/alpha/feat-x"),
                branch: Some("feat/x".into()),
                head: String::new(),
            },
        ];
        let row = |pid: u32, cwd: &str| switchbard_core::AgentProcessRow {
            pid,
            process_identity: None,
            kind: AgentProcessKind::Claude,
            cwd: Some(PathBuf::from(cwd)),
            started_unix: None,
            pgid: None,
            session_id: None,
            name: None,
            activity: AgentActivity::Idle,
        };
        let rows = vec![
            row(1, "/w/alpha/src"),
            row(2, "/w/beta"),
            row(3, "/w/.worktrees/alpha/feat-x"),
            row(4, "/w/alphabet"),
        ];
        let sessions = attribute_to_repo(&rows, &worktrees);
        let pids: Vec<u32> = sessions.iter().map(|s| s.pid).collect();
        assert_eq!(pids, vec![1, 3]);
        assert_eq!(sessions[1].worktree_branch.as_deref(), Some("feat/x"));
    }

    #[test]
    fn short_age_uses_the_largest_whole_unit() {
        assert_eq!(short_age(Duration::from_secs(4)), "4s");
        assert_eq!(short_age(Duration::from_secs(61)), "1m");
        assert_eq!(short_age(Duration::from_secs(7_200)), "2h");
        assert_eq!(short_age(Duration::from_secs(200_000)), "2d");
    }
}
