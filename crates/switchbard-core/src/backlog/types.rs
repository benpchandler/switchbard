//! Struct/enum definitions for Backlog.md task data — no parsing, no CLI
//! invocation, just the shapes and their small pure helper methods. See
//! `super::parse` for turning task markdown into these, and
//! `super::mutations` for CLI calls that change them on disk.

use std::path::PathBuf;

pub const BACKLOG_STATUSES: &[&str] = &["To Do", "In Progress", "Done"];
pub const BACKLOG_PRIORITIES: &[&str] = &["high", "medium", "low"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BacklogProject {
    pub root: PathBuf,
    pub cli_path: Option<PathBuf>,
    pub tasks: Vec<BacklogTask>,
    pub warnings: Vec<String>,
    /// Milliseconds since the Unix epoch when this snapshot was read from
    /// disk (`unix_now`) — millisecond, not second, precision specifically
    /// so `workers::merge_backlog_projects` can use it to detect a stale
    /// scan racing a fresher single-project refresh (TASK-30 fix):
    /// two loads of the same project easily land in the same *second*
    /// (a periodic multi-repo scan and a just-completed mutation's own
    /// targeted reload), which would make a second-granularity timestamp
    /// tie exactly in the case that matters most.
    pub loaded_at_unix: u64,
    /// This project's own configured status list (`backlog/config.yml`'s
    /// `statuses:` array), in the order the project itself declares —
    /// e.g. budget's own config declares `["Icebox", "To Do", "In
    /// Progress", "In Review", "Done"]`. TASK-25 (owner-requested UX): the
    /// Board lens's column set is the union of every tracked project's own
    /// list, not just statuses a task happens to carry right now, so a
    /// repo-specific status like Icebox shows even with zero Icebox tasks
    /// in the current scope. Empty if `config.yml` is missing or its
    /// `statuses` key is absent/malformed — never fatal, since every
    /// existing behavior (BACKLOG_STATUSES + statuses actually present on a
    /// task) is unaffected either way.
    pub configured_statuses: Vec<String>,
}

impl BacklogProject {
    pub fn cli_available(&self) -> bool {
        self.cli_path.is_some()
    }

    pub fn active_task_count(&self) -> usize {
        self.tasks
            .iter()
            .filter(|task| task.source == BacklogTaskSource::Active)
            .count()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BacklogTaskSource {
    Active,
    Completed,
    Draft,
    Archived,
}

impl BacklogTaskSource {
    pub fn label(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Completed => "completed",
            Self::Draft => "draft",
            Self::Archived => "archived",
        }
    }

    fn editable(self) -> bool {
        matches!(self, Self::Active)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BacklogTask {
    pub id: String,
    pub title: String,
    pub status: String,
    pub priority: String,
    pub assignees: Vec<String>,
    pub labels: Vec<String>,
    pub dependencies: Vec<String>,
    pub references: Vec<String>,
    pub milestone: Option<String>,
    pub parent: Option<String>,
    pub created_date: Option<String>,
    pub updated_date: Option<String>,
    pub description: String,
    pub implementation_plan: String,
    pub implementation_notes: String,
    pub final_summary: String,
    pub acceptance_criteria: Vec<BacklogChecklistItem>,
    pub definition_of_done: Vec<BacklogChecklistItem>,
    pub source: BacklogTaskSource,
    pub path: PathBuf,
}

impl BacklogTask {
    pub fn editable(&self) -> bool {
        self.source.editable()
    }

    pub fn acceptance_done_count(&self) -> usize {
        self.acceptance_criteria
            .iter()
            .filter(|item| item.checked)
            .count()
    }

    pub fn dod_done_count(&self) -> usize {
        self.definition_of_done
            .iter()
            .filter(|item| item.checked)
            .count()
    }

    /// `true` for the statuses the burndown/statistics views treat as
    /// finished. Mirrors `sort::task_is_completed`'s GUI-side notion but
    /// lives in core so `backlog_stats` (which has no GUI dependency) can
    /// share the exact same definition rather than re-deriving it.
    pub fn is_done(&self) -> bool {
        self.source == BacklogTaskSource::Completed || self.status.eq_ignore_ascii_case("done")
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BacklogChecklistItem {
    pub index: usize,
    pub checked: bool,
    pub text: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BacklogTaskPatch {
    pub title: Option<String>,
    pub description: Option<String>,
    pub status: Option<String>,
    pub priority: Option<String>,
    pub labels: Option<Vec<String>>,
    pub assignees: Option<Vec<String>>,
    pub dependencies: Option<Vec<String>>,
    /// `--ref` replaces the whole references list per invocation (verified
    /// against the live CLI — it is a set operation, not additive), so
    /// "adding" a reference from the UI means submitting the full list with
    /// the new entry appended, same shape as `labels`/`dependencies`.
    pub references: Option<Vec<String>>,
    pub implementation_plan: Option<String>,
    /// `Some(name)` assigns the milestone; `None` with `clear_milestone` unset
    /// leaves it untouched. Assign and clear are mutually exclusive — callers
    /// that want to clear set `clear_milestone` instead of this field.
    pub milestone: Option<String>,
    /// Clears the task's milestone assignment (`--clear-milestone`). Ignored
    /// if `milestone` is also set (assigning wins) — `is_empty` doesn't need
    /// to police that; `edit_backlog_task` only ever receives one or the
    /// other from the UI layer.
    pub clear_milestone: bool,
}

impl BacklogTaskPatch {
    pub fn is_empty(&self) -> bool {
        self.title.is_none()
            && self.description.is_none()
            && self.status.is_none()
            && self.priority.is_none()
            && self.labels.is_none()
            && self.assignees.is_none()
            && self.dependencies.is_none()
            && self.references.is_none()
            && self.implementation_plan.is_none()
            && self.milestone.is_none()
            && !self.clear_milestone
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewBacklogTask {
    pub title: String,
    pub description: String,
    pub status: String,
    pub priority: String,
    pub acceptance_criteria: Vec<String>,
    /// Parent task id (task-17's create-subtask), passed as `-p`/`--parent`.
    pub parent: Option<String>,
    /// QA parity matrix LOW gap: set at creation time via `-l`, same
    /// comma-joined shape `BacklogTaskPatch::labels` uses for edit.
    pub labels: Vec<String>,
    /// Passed as `-a`, comma-joined (verified against `backlog task create
    /// --help`, same flag `edit_backlog_task` already uses for
    /// `BacklogTaskPatch::assignees`).
    pub assignees: Vec<String>,
    /// Passed as `-m` (verified against `backlog task create --help`).
    pub milestone: Option<String>,
    /// Passed as `--depends-on`, comma-joined (verified against `backlog
    /// task create --help`; same flag `edit_backlog_task` uses for
    /// `BacklogTaskPatch::dependencies`).
    pub dependencies: Vec<String>,
}
