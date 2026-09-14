//! Automatic session checkpoints and recognizable, deliberately restored history.

use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use super::{resume, App};
use crate::page::Page;
use crate::paint::PaintRule;
use crate::picker::{Payload, PickOption, PickerPurpose};
use crate::views::ViewState;

const CHECKPOINT_INTERVAL: Duration = Duration::from_secs(30);

impl App {
    /// One status line at session start, never repeated per tick (TASK-227).
    /// After a self-restart into a new binary: "updated to `<sha>` `<branch>`",
    /// naming the previous build when `prev_build` carries one. Otherwise,
    /// whatever `scripts/install-switchbard.sh` last left in
    /// `auto_install_dir` - an active hold, or a refused attempt - if
    /// anything. Call once, after `restore_session`.
    pub fn show_startup_banner(&mut self, restarted: bool, prev_build: Option<&str>) {
        if restarted {
            self.status = crate::auto_install::updated_status_line(prev_build);
            return;
        }
        if let Some(notice) = crate::auto_install::pending_notice(
            self.auto_install_dir.as_deref(),
            chrono::Utc::now(),
        ) {
            self.status = notice;
        }
    }

    /// An explicit restart handoff wins even when the original launch was fresh.
    pub fn restore_session(&mut self, restart: Option<&str>, fresh: bool) {
        // Opting out of restoration does not authorize discarding unreadable data.
        if let resume::Restored::Record(record) = self.resume_store.read() {
            if let Err(error) = validate_resume_views(&record, &self.registry) {
                self.resume_store.block_writes(error);
            }
        }
        if restart.is_some() {
            self.resume_from(restart);
            return;
        }
        if fresh {
            return;
        }
        match self.resume_store.read() {
            resume::Restored::Absent => {}
            resume::Restored::Record(record) => self.restore_cold_record(record),
            resume::Restored::Unreadable => self.fail(
                "resume unreadable; saved default opened; repair the .resume file and reopen"
                    .into(),
            ),
        }
    }

    fn restore_cold_record(&mut self, record: resume::ResumeRecord) {
        let valid = validate_resume_views(&record, &self.registry);
        if let Err(error) = valid {
            self.resume_store.block_writes(error.clone());
            self.fail(format!(
                "resume view unreadable; repair .resume and reopen: {error}"
            ));
            return;
        }
        self.resume_from(Some(&record.encode()));
        self.status = "resumed last view · v h history · --fresh opens saved default".into();
    }

    /// Timer and all graceful exits use this one capture path. Slots are never written.
    pub fn checkpoint_session(&mut self) -> Result<(), String> {
        let now = epoch_seconds();
        let mut errors = Vec::new();
        if let Err(error) = self.checkpoint_resume() {
            errors.push(format!("resume not saved: {error}"));
        }
        let (tasks, prs) = if self.page == Page::PullRequests {
            (&self.inactive_state, &self.state)
        } else {
            (&self.state, &self.inactive_state)
        };
        for (page, state) in [(Page::Tasks, tasks), (Page::PullRequests, prs)] {
            if let Err(error) = self.history.capture(page, state, &self.registry, now) {
                errors.push(format!("history not saved: {error}"));
            }
        }
        if errors.is_empty() {
            return Ok(());
        }
        let error = errors.join("; ");
        self.fail(error.clone());
        Err(error)
    }

    fn checkpoint_resume(&mut self) -> Result<(), String> {
        let state = self.resume_state();
        let resume::Restored::Record(record) = resume::decode(Some(&state)) else {
            return Err("outgoing resume record is unreadable".into());
        };
        validate_resume_views(&record, &self.registry)?;
        self.resume_store.checkpoint(&state)
    }

    /// A monotonic deadline keeps rapid input from starving persistence.
    pub fn checkpoint_if_due(&mut self, now: Instant) {
        if now < self.checkpoint_due {
            return;
        }
        self.checkpoint_due = now + CHECKPOINT_INTERVAL;
        match self.checkpoint_session() {
            Ok(()) | Err(_) => {} // Errors already have status and telemetry.
        }
    }

    pub(super) fn open_history(&mut self) {
        let now = epoch_seconds();
        let options = self
            .history
            .entries(self.page, now)
            .into_iter()
            .filter_map(|entry| {
                let state = entry.view(&self.registry).ok()?;
                Some(PickOption {
                    label: format!(
                        "{} · {}",
                        relative_time(now, entry.visited_at),
                        history_label(&state, &self.registry)
                    ),
                    count: 0,
                    key: None,
                    payload: Payload::HistoryView(entry.view_lua().to_owned()),
                })
            })
            .collect::<Vec<_>>();
        let empty = options.is_empty();
        self.open_picker(PickerPurpose::History, options);
        self.status = if empty {
            "No view history yet; captured every 30 seconds and on exit".into()
        } else {
            "Restore a view, then v s <number> to keep it in a slot".into()
        };
    }

    pub(super) fn restore_history(&mut self, record: &str) {
        match ViewState::try_from_lua(record, &self.registry) {
            Ok(mut view) => {
                view.sanitize(self.page, &self.registry);
                self.state = view;
                self.view = crate::views::MAX_SLOTS;
                self.refilter();
                self.status = "history restored · v s <number> saves to a slot".into();
                self.telemetry.record("action", "view_history_restore");
            }
            Err(error) => self.fail(format!("history entry unreadable: {error}")),
        }
    }
}

fn history_label(state: &ViewState, registry: &crate::columns::ColumnRegistry) -> String {
    let mut label = state.label(registry);
    let painted = state
        .paint
        .iter()
        .map(|rule| match rule {
            PaintRule::ByColumn { column, colors } => format!(
                "by {} [{}]",
                column.name(registry),
                colors
                    .iter()
                    .map(|(value, color)| format!("{value}:{color}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            PaintRule::Rows { filter, color } => format!("rows {filter}:{color}"),
            PaintRule::Column { column, color } => {
                format!("column {}:{color}", column.name(registry))
            }
        })
        .collect::<Vec<_>>();
    if !painted.is_empty() {
        label.push_str(&format!(" · painted {}", painted.join(", ")));
    }
    format!("{label} · {} cols", state.columns.len())
}

pub(super) fn epoch_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn relative_time(now: u64, then: u64) -> String {
    match now.saturating_sub(then) {
        0..60 => "just now".into(),
        age @ 60..3600 => format!("{}m ago", age / 60),
        age @ 3600..86400 => format!("{}h ago", age / 3600),
        age => format!("{}d ago", age / 86400),
    }
}

fn validate_resume_views(
    record: &resume::ResumeRecord,
    registry: &crate::columns::ColumnRegistry,
) -> Result<(), String> {
    [&record.task_view, &record.pr_view]
        .into_iter()
        .filter(|view| !view.is_empty())
        .try_for_each(|view| ViewState::try_from_lua(view, registry).map(|_| ()))
}
