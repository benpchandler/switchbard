//! Struct/enum definitions for Backlog.md task data — no parsing, no CLI
//! invocation, just the shapes and their small pure helper methods. See
//! `super::parse` for turning task markdown into these, and
//! `super::mutations` for CLI calls that change them on disk.

use std::collections::BTreeSet;
use std::path::PathBuf;

/// The three-status subset offered as one-click actions on a List row. This
/// is an *affordance*, not a vocabulary claim — the full set a user can pick
/// from is [`STANDARD_STATUSES`]. Five buttons per row would crowd the row;
/// the rarer transitions live in the detail rail's dropdown.
pub const BACKLOG_STATUSES: &[&str] = &["To Do", "In Progress", "Done"];
pub const BACKLOG_PRIORITIES: &[&str] = &["high", "medium", "low"];

/// The standardized cross-repo status vocabulary (owner decision 2026-08-06):
/// every tracked project offers exactly these, whatever its own
/// `backlog/config.yml` happens to declare.
///
/// Chosen from what the tracked repos already used rather than invented: a
/// survey of all 8 configured repos found `budget` declaring precisely this
/// five-value list and the other four backlog-bearing repos (CambridgeKitchens,
/// MusicProduction, switchbard, hub) declaring an exact *subset*
/// (`To Do`/`In Progress`/`Done`). Standardizing on the superset therefore
/// reclassifies nothing — 322 of 323 existing tasks already sit on a value in
/// this list. `In Review` in particular is the dispatch pipeline's
/// "agent finished, a human should look" state (see `crate::dispatch`); it was
/// already declared by `budget` and simply never used, so the lifecycle needed
/// no new vocabulary.
///
/// The one holdout is a single MusicProduction task on `Backlog`, which no
/// repo declares. It is deliberately NOT erased: [`CANONICAL_STATUS_ORDER`]
/// still positions it so the task stays visible and sensibly sorted until
/// someone moves it.
pub const STANDARD_STATUSES: &[&str] = &["Icebox", "To Do", "In Progress", "In Review", "Done"];

/// The owner's preferred kanban ordering for any status name this app has
/// ever seen, whether or not it's one of `BACKLOG_STATUSES` — a leading
/// "Backlog"/"Icebox" pair for pre-triage work, then the standard flow,
/// ending in "Done". Anything outside this list sorts alphabetically after
/// it (see `ordered_status_vocabulary`), so a repo's genuinely nonstandard
/// status still gets a stable, deterministic position rather than being
/// dropped.
pub const CANONICAL_STATUS_ORDER: &[&str] = &[
    "Backlog",
    "Icebox",
    "To Do",
    "In Progress",
    "In Review",
    "Done",
];

/// Every status the scoped projects *actually have* — their declared
/// `backlog/config.yml` lists, plus any status a task in scope currently
/// carries (so an ad hoc value still gets a column rather than vanishing).
///
/// # Why this no longer seeds [`STANDARD_STATUSES`]
///
/// It used to, under the 2026-08-05 request that "all projects should share a
/// common set of statuses across every view". Offering a shared vocabulary is
/// a fine goal; asserting one the repos don't have is not. The `backlog` CLI
/// validates every write against the project's own config, so seeding here
/// made the Board offer `Icebox` for a repo declaring only the standard trio,
/// and the drop failed with `Invalid status: Icebox. Valid statuses are: To
/// Do, In Progress, Done`. Worse, `dispatch` releases finished runs to `In
/// Review` and swallows the write error by design, so in three of four
/// tracked repos that move silently never happened.
///
/// **What the board shows now matches what the repo declares** (owner
/// decision, 2026-08-28). Standardization did not go away — it moved from an
/// assertion to an offer: [`missing_standard_statuses`] finds the gap and the
/// UI proposes writing it into the repo, which makes the shared vocabulary
/// true rather than assumed.
pub fn ordered_status_vocabulary<'a>(
    projects: impl IntoIterator<Item = &'a BacklogProject>,
) -> Vec<String> {
    let mut set: BTreeSet<String> = BTreeSet::new();
    for project in projects {
        for status in &project.configured_statuses {
            set.insert(status.clone());
        }
        for task in &project.tasks {
            set.insert(task.status.clone());
        }
    }

    order_statuses(set)
}

/// `CANONICAL_STATUS_ORDER` first, anything else alphabetically after, so a
/// repo's genuinely nonstandard status still gets a stable position rather
/// than being dropped. Shared by every list of statuses this crate hands out —
/// two orderings would be two vocabularies again.
pub(super) fn order_statuses_public(set: BTreeSet<String>) -> Vec<String> {
    order_statuses(set)
}

fn order_statuses(set: BTreeSet<String>) -> Vec<String> {
    let mut canonical: Vec<String> = Vec::new();
    let mut extra: Vec<String> = Vec::new();
    for status in set {
        if CANONICAL_STATUS_ORDER
            .iter()
            .any(|c| c.eq_ignore_ascii_case(&status))
        {
            canonical.push(status);
        } else {
            extra.push(status);
        }
    }
    canonical.sort_by_key(|status| {
        CANONICAL_STATUS_ORDER
            .iter()
            .position(|c| c.eq_ignore_ascii_case(status))
            .unwrap_or(CANONICAL_STATUS_ORDER.len())
    });
    extra.sort();
    canonical.extend(extra);
    canonical
}

/// The statuses a task in this project may legally be moved *to*.
///
/// Deliberately narrower than [`ordered_status_vocabulary`], which also
/// includes statuses tasks merely carry: a stray `Backlog` value on one task
/// is a thing to render, not a destination to offer. The `backlog` CLI is the
/// authority and it accepts exactly the config's list, so this mirrors it.
pub fn assignable_statuses(project: &BacklogProject) -> Vec<String> {
    order_statuses(project.configured_statuses.iter().cloned().collect())
}

/// Which of [`STANDARD_STATUSES`] this project's `backlog/config.yml` omits.
///
/// Empty means the repo already offers the shared vocabulary. Non-empty is
/// the gap the UI offers to close — see `ordered_status_vocabulary` for why
/// closing it is now an offer rather than an assumption.
pub fn missing_standard_statuses(project: &BacklogProject) -> Vec<String> {
    STANDARD_STATUSES
        .iter()
        .filter(|standard| {
            !project
                .configured_statuses
                .iter()
                .any(|declared| declared.eq_ignore_ascii_case(standard))
        })
        .map(|s| (*s).to_string())
        .collect()
}

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
    /// Acceptance criteria to *append* (`--ac`, repeatable), leaving every
    /// existing criterion's text and checked state alone. Distinct from the
    /// CLI's `--acceptance-criteria`, which replaces the whole list — the
    /// only writer today is `crate::refine`, whose entire contract is that a
    /// human-authored, possibly already-checked criterion is never disturbed
    /// by an agent's suggestions.
    pub append_acceptance_criteria: Vec<String>,
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
            && self.append_acceptance_criteria.is_empty()
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

#[cfg(test)]
mod tests {
    use super::*;

    fn project(configured_statuses: &[&str], task_statuses: &[&str]) -> BacklogProject {
        BacklogProject {
            root: PathBuf::from("/fixture"),
            cli_path: None,
            tasks: task_statuses
                .iter()
                .enumerate()
                .map(|(i, status)| BacklogTask {
                    id: format!("TASK-{}", i + 1),
                    title: "fixture".to_string(),
                    status: (*status).to_string(),
                    priority: "medium".to_string(),
                    assignees: vec![],
                    labels: vec![],
                    dependencies: vec![],
                    references: vec![],
                    milestone: None,
                    parent: None,
                    created_date: None,
                    updated_date: None,
                    description: String::new(),
                    implementation_plan: String::new(),
                    implementation_notes: String::new(),
                    final_summary: String::new(),
                    acceptance_criteria: vec![],
                    definition_of_done: vec![],
                    source: BacklogTaskSource::Active,
                    path: PathBuf::from("/fixture/backlog/tasks/fixture.md"),
                })
                .collect(),
            warnings: vec![],
            loaded_at_unix: 0,
            configured_statuses: configured_statuses.iter().map(|s| s.to_string()).collect(),
        }
    }

    /// Both of the next two tests assert the *reverse* of what they used to
    /// (owner decision, 2026-08-28). They previously pinned the
    /// standardization guarantee — the vocabulary was seeded from
    /// `STANDARD_STATUSES`, so every surface offered `Icebox` and `In Review`
    /// whatever a repo declared. That made the app confident about statuses
    /// the `backlog` CLI would then refuse to write, which is how a board drag
    /// produced `Invalid status: Icebox` and how `dispatch` silently failed to
    /// release runs to `In Review` in three of four repos.
    ///
    /// What shows now matches what the repo declares. The standardization goal
    /// survives as [`missing_standard_statuses`] plus a UI offer to write the
    /// gap into `config.yml` — making the shared vocabulary true rather than
    /// assumed.
    #[test]
    fn nothing_in_scope_offers_nothing() {
        assert!(
            ordered_status_vocabulary(std::iter::empty()).is_empty(),
            "with no project in scope there is no repo to be truthful about"
        );
    }

    #[test]
    fn a_project_declaring_only_the_narrow_trio_offers_only_the_trio() {
        let p = project(&["To Do", "In Progress", "Done"], &[]);
        assert_eq!(
            ordered_status_vocabulary([&p]),
            vec!["To Do", "In Progress", "Done"],
        );
        assert_eq!(
            missing_standard_statuses(&p),
            vec!["Icebox", "In Review"],
            "and the gap is reported, so the UI can offer to close it"
        );
    }

    /// The move targets a task in this project may legally take: the config's
    /// list and nothing else, because that is exactly what the CLI accepts.
    /// A status merely *carried* by some task is not a destination — offering
    /// it would recreate the rejected-write bug in a subtler place.
    #[test]
    fn assignable_statuses_are_the_configs_and_not_a_tasks_stray_value() {
        let p = project(&["To Do", "Done"], &["Blocked"]);
        assert_eq!(assignable_statuses(&p), vec!["To Do", "Done"]);
        assert!(
            ordered_status_vocabulary([&p]).contains(&"Blocked".to_string()),
            "the stray value still gets a column so its task stays visible"
        );
    }

    #[test]
    fn includes_a_projects_configured_statuses_even_with_zero_matching_tasks() {
        let p = project(
            &["Icebox", "To Do", "In Progress", "In Review", "Done"],
            &[],
        );
        let vocab = ordered_status_vocabulary([&p]);
        assert_eq!(
            vocab,
            vec!["Icebox", "To Do", "In Progress", "In Review", "Done"],
            "Icebox and In Review should appear even though no task carries them"
        );
    }

    #[test]
    fn includes_a_nonstandard_status_actually_present_on_a_task() {
        let p = project(&[], &["Blocked"]);
        assert_eq!(
            ordered_status_vocabulary([&p]),
            vec!["Blocked"],
            "a genuinely ad hoc task status still gets a column, or its task vanishes"
        );
    }

    #[test]
    fn orders_canonically_regardless_of_which_project_declared_what() {
        // "Backlog" is the one legacy value no repo declares (a single
        // MusicProduction task still carries it), so it is deliberately not in
        // STANDARD_STATUSES — but CANONICAL_STATUS_ORDER still places it ahead
        // of the standardized set rather than dumping it in the alphabetical
        // tail, which is what keeps that task sensibly sorted until it moves.
        let a = project(&["Backlog"], &[]);
        let b = project(&["In Review"], &[]);
        assert_eq!(
            ordered_status_vocabulary([&a, &b]),
            vec!["Backlog", "In Review"],
        );
    }

    #[test]
    fn extra_nonstandard_statuses_sort_alphabetically_after_the_canonical_set() {
        let p = project(&["To Do"], &["Zeta", "Alpha"]);
        let vocab = ordered_status_vocabulary([&p]);
        assert_eq!(vocab, vec!["To Do", "Alpha", "Zeta"]);
    }
}
