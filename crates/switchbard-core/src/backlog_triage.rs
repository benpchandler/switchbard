//! Cross-repo Backlog triage ranking — the domain logic behind the unified
//! Backlog view's "All projects" scope.
//!
//! Two layers, deliberately separated per the repo's Rule 5 (validation
//! depth follows the named threat — here the "threat" is just "keep the
//! ranking testable"): [`triage_rank`] and [`OrderingOverlay::parse`] are
//! pure functions with no IO; [`load_ordering_overlay`] and [`find_hub_repo`]
//! are the thin IO boundary that feeds them from disk.
//!
//! ## Due dates
//!
//! The ranking contract is overlay-rank → overdue → due-today → priority →
//! blocked → age → repo name. As of Backlog CLI v1.47 (`backlog task edit
//! --help`), Backlog.md's task schema has **no due-date field** — nothing in
//! a task's frontmatter or sections carries one. [`TriageDue`] and the
//! overdue/due-today tiers are still implemented and unit-tested here
//! because the ranking contract names them explicitly (task-10 AC #2); every
//! entry built from a real [`BacklogTask`] via [`triage_entry_from_task`]
//! currently passes [`TriageDue::None`], so those tiers are inert in
//! production until the CLI/schema grows a due-date concept. Wiring real due
//! dates is future work, not scope of this module.
//!
//! ## Blocked (task-18)
//!
//! `blocked` (from `backlog_relations::is_blocked`) sits *after* priority
//! and *before* age: it's a fine-grained tiebreaker within a priority tier
//! ("you can't act on this one right now, so the otherwise-equal task you
//! *can* act on goes first"), not a tier that overrides priority itself — an
//! overdue high-priority blocked task still outranks a due-today low-
//! priority unblocked one. The overlay and due/priority tiers are about how
//! important a task is; blocked is about whether it's actionable *today*,
//! which only matters once importance is already tied.

use crate::backlog::BacklogTask;
use std::cmp::Ordering;
use std::fs;
use std::path::{Path, PathBuf};

/// One task's ranking-relevant facts, extracted from a `BacklogProject` by
/// the caller. Deliberately decoupled from `BacklogTask`/`BacklogProject` so
/// the ranking function itself has no IO and no markdown/YAML parsing
/// dependency — see the module doc.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TriageEntry {
    /// The project's worktree-root key, exactly as used elsewhere backlog
    /// state is keyed (`HiveApp::backlog_projects`). Disambiguates tasks
    /// when two tracked repos happen to share a `repo` name.
    pub project_key: PathBuf,
    /// Repo identity as it appears in `ordering.yml` ("repo-dir-name") and
    /// as the row's repo badge — `Repo::name`.
    pub repo: String,
    pub task_id: String,
    pub priority: TriagePriority,
    pub due: TriageDue,
    /// From `backlog_relations::is_blocked` — see the module doc's "Blocked"
    /// section for where this sits in the ranking chain.
    pub blocked: bool,
    /// Unix seconds the task was created. Unknown/unparseable dates use
    /// `u64::MAX` (see [`triage_entry_from_task`]) so an unknown age sinks to
    /// the back of its tier rather than wrongly dominating the front.
    pub age_unix: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TriagePriority {
    High,
    Medium,
    Low,
    Unknown,
}

impl TriagePriority {
    pub fn from_str_loose(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "high" => Self::High,
            "medium" => Self::Medium,
            "low" => Self::Low,
            _ => Self::Unknown,
        }
    }

    fn rank(self) -> u8 {
        match self {
            Self::High => 0,
            Self::Medium => 1,
            Self::Low => 2,
            Self::Unknown => 3,
        }
    }
}

/// See the module doc's "Due dates" section — always `None` for entries
/// built from real `BacklogTask` data today.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TriageDue {
    Overdue,
    DueToday,
    None,
}

impl TriageDue {
    fn rank(self) -> u8 {
        match self {
            Self::Overdue => 0,
            Self::DueToday => 1,
            Self::None => 2,
        }
    }
}

/// Build a [`TriageEntry`] from a real task. `repo` is the tracked `Repo`'s
/// `name` (the ordering.yml / badge identity); `project_key` is the
/// project's worktree-root path as used in `HiveApp::backlog_projects`;
/// `project` is `task`'s own project, needed to resolve its dependencies'
/// statuses for the blocked computation.
pub fn triage_entry_from_task(
    project_key: PathBuf,
    repo: &str,
    task: &BacklogTask,
    project: &crate::backlog::BacklogProject,
) -> TriageEntry {
    TriageEntry {
        project_key,
        repo: repo.to_string(),
        task_id: task.id.clone(),
        priority: TriagePriority::from_str_loose(&task.priority),
        due: TriageDue::None,
        blocked: crate::backlog_relations::is_blocked(task, project),
        age_unix: task
            .created_date
            .as_deref()
            .and_then(parse_backlog_datetime_unix)
            .unwrap_or(u64::MAX),
    }
}

/// Backlog.md stores `created_date`/`updated_date` as `"YYYY-MM-DD HH:MM"`
/// (see `backlog::parse_task_file` / the frontmatter in any
/// `backlog/tasks/*.md`). Returns `None` for anything else rather than
/// guessing. Seconds precision, unlike `backlog::parse_backlog_day`'s day
/// granularity (burndown/relations only need "which day"); this module's own
/// age tiebreak needs finer resolution, and the Board lens's per-card age
/// label (task-15 parity gap) reuses it via `humanize_age` for the same
/// reason — a card updated an hour ago shouldn't look as stale as one from
/// yesterday.
pub fn parse_backlog_datetime_unix(value: &str) -> Option<u64> {
    chrono::NaiveDateTime::parse_from_str(value.trim(), "%Y-%m-%d %H:%M")
        .ok()
        .map(|dt| dt.and_utc().timestamp())
        .and_then(|secs| u64::try_from(secs).ok())
}

/// The global cross-repo priority overlay (`<hub repo>/ordering.yml`).
/// Entries are `"<repo>:<task-id>"` strings in explicit rank order, highest
/// priority first; a task absent from the list falls through to the
/// computed triage order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OrderingOverlay {
    ranked: Vec<String>,
}

#[derive(serde::Deserialize)]
struct OrderingYaml {
    #[serde(default)]
    ranked: Vec<String>,
}

impl OrderingOverlay {
    pub fn empty() -> Self {
        Self::default()
    }

    /// Parse `ordering.yml`'s contents. Pure — no IO. A malformed document
    /// (invalid YAML, or `ranked` present with the wrong shape) falls back to
    /// an empty overlay plus a non-fatal warning describing the parse error,
    /// per task-10 AC #3.
    pub fn parse(yaml: &str) -> (Self, Option<String>) {
        match serde_yaml::from_str::<OrderingYaml>(yaml) {
            Ok(parsed) => (
                Self {
                    ranked: parsed.ranked,
                },
                None,
            ),
            Err(e) => (
                Self::empty(),
                Some(format!("ordering.yml is malformed, ignoring overlay: {e}")),
            ),
        }
    }

    /// Explicit rank of `repo:task_id`, if the overlay names it. Lower is
    /// higher priority. Exact, case-sensitive match against `Repo::name` and
    /// `BacklogTask::id` as stored — consistent with how both are compared
    /// everywhere else in this codebase.
    fn rank_of(&self, repo: &str, task_id: &str) -> Option<usize> {
        let key = format!("{repo}:{task_id}");
        self.ranked.iter().position(|entry| entry == &key)
    }
}

/// Locate the hub repo among tracked repo roots: the first one containing an
/// `ordering.yml` file at its root. Returns `None` when no tracked repo has
/// one — callers treat that as "empty overlay", not an error.
pub fn find_hub_repo<'a>(repo_roots: impl IntoIterator<Item = &'a Path>) -> Option<PathBuf> {
    repo_roots
        .into_iter()
        .find(|root| root.join("ordering.yml").is_file())
        .map(Path::to_path_buf)
}

/// IO boundary: read and parse `<hub_root>/ordering.yml`. A missing file is
/// an empty overlay with **no** warning (per AC #3 — absence is the expected
/// steady state until a hub repo exists); a present-but-malformed file is an
/// empty overlay **with** a warning.
pub fn load_ordering_overlay(hub_root: &Path) -> (OrderingOverlay, Option<String>) {
    match fs::read_to_string(hub_root.join("ordering.yml")) {
        Ok(text) => OrderingOverlay::parse(&text),
        Err(_) => (OrderingOverlay::empty(), None),
    }
}

/// Sort `entries` by the triage contract: explicit overlay rank first, then
/// overdue > due-today > priority > unblocked-before-blocked > age (older
/// first) > repo name > task id. Pure — no IO, no locking, safe to unit test
/// exhaustively and to call from the UI thread on every frame.
pub fn triage_rank(entries: &[TriageEntry], overlay: &OrderingOverlay) -> Vec<TriageEntry> {
    let mut ranked = entries.to_vec();
    ranked.sort_by(|a, b| compare_entries(a, b, overlay));
    ranked
}

fn compare_entries(a: &TriageEntry, b: &TriageEntry, overlay: &OrderingOverlay) -> Ordering {
    let a_rank = overlay.rank_of(&a.repo, &a.task_id);
    let b_rank = overlay.rank_of(&b.repo, &b.task_id);
    match (a_rank, b_rank) {
        (Some(ra), Some(rb)) => return ra.cmp(&rb),
        // Overlay-ranked entries always outrank unranked ones, regardless of
        // where an unranked entry would otherwise fall.
        (Some(_), None) => return Ordering::Less,
        (None, Some(_)) => return Ordering::Greater,
        (None, None) => {}
    }
    a.due
        .rank()
        .cmp(&b.due.rank())
        .then_with(|| a.priority.rank().cmp(&b.priority.rank()))
        .then_with(|| a.blocked.cmp(&b.blocked)) // false (unblocked) < true (blocked)
        .then_with(|| a.age_unix.cmp(&b.age_unix))
        .then_with(|| a.repo.cmp(&b.repo))
        .then_with(|| a.task_id.cmp(&b.task_id))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(repo: &str, task_id: &str) -> TriageEntry {
        TriageEntry {
            project_key: PathBuf::from(format!("/repos/{repo}")),
            repo: repo.to_string(),
            task_id: task_id.to_string(),
            priority: TriagePriority::Medium,
            due: TriageDue::None,
            blocked: false,
            age_unix: 1_000,
        }
    }

    fn ids(entries: &[TriageEntry]) -> Vec<&str> {
        entries.iter().map(|e| e.task_id.as_str()).collect()
    }

    #[test]
    fn overlay_rank_wins_over_every_computed_tier() {
        let mut low_priority_but_ranked = entry("repo-a", "TASK-9");
        low_priority_but_ranked.priority = TriagePriority::Low;
        low_priority_but_ranked.age_unix = 999_999; // very new

        let mut high_priority_unranked = entry("repo-b", "TASK-1");
        high_priority_unranked.priority = TriagePriority::High;
        high_priority_unranked.due = TriageDue::Overdue;
        high_priority_unranked.age_unix = 0; // very old

        let overlay = OrderingOverlay::parse("ranked: [\"repo-a:TASK-9\"]").0;
        let ranked = triage_rank(&[high_priority_unranked, low_priority_but_ranked], &overlay);

        assert_eq!(ids(&ranked), vec!["TASK-9", "TASK-1"]);
    }

    #[test]
    fn overlay_order_is_respected_among_multiple_ranked_entries() {
        let a = entry("repo-a", "TASK-1");
        let b = entry("repo-a", "TASK-2");
        let c = entry("repo-a", "TASK-3");
        let overlay = OrderingOverlay::parse(
            "ranked: [\"repo-a:TASK-3\", \"repo-a:TASK-1\", \"repo-a:TASK-2\"]",
        )
        .0;

        let ranked = triage_rank(&[a, b, c], &overlay);

        assert_eq!(ids(&ranked), vec!["TASK-3", "TASK-1", "TASK-2"]);
    }

    #[test]
    fn overdue_beats_due_today_which_beats_no_due_date() {
        let mut overdue = entry("repo-a", "TASK-1");
        overdue.due = TriageDue::Overdue;
        let mut due_today = entry("repo-a", "TASK-2");
        due_today.due = TriageDue::DueToday;
        let no_due = entry("repo-a", "TASK-3");

        let ranked = triage_rank(&[no_due, due_today, overdue], &OrderingOverlay::empty());

        assert_eq!(ids(&ranked), vec!["TASK-1", "TASK-2", "TASK-3"]);
    }

    #[test]
    fn blocked_sinks_below_unblocked_within_the_same_priority_tier() {
        let mut blocked = entry("repo-a", "TASK-1");
        blocked.blocked = true;
        let unblocked = entry("repo-a", "TASK-2");

        let ranked = triage_rank(&[blocked, unblocked], &OrderingOverlay::empty());

        assert_eq!(ids(&ranked), vec!["TASK-2", "TASK-1"]);
    }

    #[test]
    fn blocked_does_not_override_a_higher_priority_tier() {
        let mut high_blocked = entry("repo-a", "TASK-1");
        high_blocked.priority = TriagePriority::High;
        high_blocked.blocked = true;
        let mut low_unblocked = entry("repo-a", "TASK-2");
        low_unblocked.priority = TriagePriority::Low;

        // Priority is checked before blocked, so a higher-priority blocked
        // task still outranks a lower-priority unblocked one.
        let ranked = triage_rank(&[low_unblocked, high_blocked], &OrderingOverlay::empty());

        assert_eq!(ids(&ranked), vec!["TASK-1", "TASK-2"]);
    }

    #[test]
    fn priority_breaks_ties_within_the_same_due_tier() {
        let mut low = entry("repo-a", "TASK-1");
        low.priority = TriagePriority::Low;
        let mut high = entry("repo-a", "TASK-2");
        high.priority = TriagePriority::High;
        let mut medium = entry("repo-a", "TASK-3");
        medium.priority = TriagePriority::Medium;

        let ranked = triage_rank(&[low, high, medium], &OrderingOverlay::empty());

        assert_eq!(ids(&ranked), vec!["TASK-2", "TASK-3", "TASK-1"]);
    }

    #[test]
    fn older_tasks_rank_before_newer_ones_at_the_same_priority() {
        let mut newer = entry("repo-a", "TASK-1");
        newer.age_unix = 2_000;
        let mut older = entry("repo-a", "TASK-2");
        older.age_unix = 500;

        let ranked = triage_rank(&[newer, older], &OrderingOverlay::empty());

        assert_eq!(ids(&ranked), vec!["TASK-2", "TASK-1"]);
    }

    #[test]
    fn repo_name_breaks_ties_when_everything_else_matches() {
        let zebra = entry("zebra-repo", "TASK-1");
        let alpha = entry("alpha-repo", "TASK-1");

        let ranked = triage_rank(&[zebra, alpha], &OrderingOverlay::empty());

        assert_eq!(
            ranked.iter().map(|e| e.repo.as_str()).collect::<Vec<_>>(),
            vec!["alpha-repo", "zebra-repo"]
        );
    }

    #[test]
    fn task_id_is_the_final_deterministic_tiebreak() {
        let b = entry("repo-a", "TASK-2");
        let a = entry("repo-a", "TASK-1");

        let ranked = triage_rank(&[b, a], &OrderingOverlay::empty());

        assert_eq!(ids(&ranked), vec!["TASK-1", "TASK-2"]);
    }

    #[test]
    fn malformed_overlay_yaml_falls_back_to_empty_with_a_warning() {
        let (overlay, warning) = OrderingOverlay::parse("ranked: \"not a list\"");

        assert_eq!(overlay, OrderingOverlay::empty());
        assert!(warning.is_some());
    }

    #[test]
    fn missing_overlay_file_is_empty_with_no_warning() {
        let dir = tempfile::tempdir().unwrap();

        let (overlay, warning) = load_ordering_overlay(dir.path());

        assert_eq!(overlay, OrderingOverlay::empty());
        assert!(warning.is_none());
    }

    #[test]
    fn present_and_well_formed_overlay_loads_from_disk() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("ordering.yml"), "ranked: [\"a:TASK-1\"]").unwrap();

        let (overlay, warning) = load_ordering_overlay(dir.path());

        assert!(warning.is_none());
        assert_eq!(overlay.rank_of("a", "TASK-1"), Some(0));
    }

    #[test]
    fn find_hub_repo_picks_the_first_tracked_repo_with_an_ordering_file() {
        let dir = tempfile::tempdir().unwrap();
        let plain_repo = dir.path().join("plain");
        let hub_repo = dir.path().join("hub");
        std::fs::create_dir_all(&plain_repo).unwrap();
        std::fs::create_dir_all(&hub_repo).unwrap();
        std::fs::write(hub_repo.join("ordering.yml"), "ranked: []").unwrap();

        let found = find_hub_repo([plain_repo.as_path(), hub_repo.as_path()]);

        assert_eq!(found, Some(hub_repo));
    }

    #[test]
    fn find_hub_repo_is_none_when_no_tracked_repo_has_the_file() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("plain")).unwrap();

        let found = find_hub_repo([dir.path().join("plain").as_path()]);

        assert_eq!(found, None);
    }

    #[test]
    fn triage_entry_from_task_defaults_unparseable_age_to_max_not_zero() {
        let task = BacklogTask {
            id: "TASK-1".to_string(),
            title: "t".to_string(),
            status: "To Do".to_string(),
            priority: "high".to_string(),
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
            source: crate::backlog::BacklogTaskSource::Active,
            path: PathBuf::from("/repos/a/backlog/tasks/task-1.md"),
        };
        let project = crate::backlog::BacklogProject {
            root: PathBuf::from("/repos/a"),
            tasks: vec![task.clone()],
            warnings: vec![],
            loaded_at_unix: 0,
            configured_statuses: vec![],
        };

        let triage = triage_entry_from_task(PathBuf::from("/repos/a"), "a", &task, &project);

        assert_eq!(triage.age_unix, u64::MAX);
        assert_eq!(triage.priority, TriagePriority::High);
        assert_eq!(triage.due, TriageDue::None);
        assert!(!triage.blocked);
    }
}
