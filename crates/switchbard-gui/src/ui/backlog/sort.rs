//! Task filtering, sorting, and the triage-ranking pipeline.
//!
//! `visible_task_rows` is the single place that turns "every tracked
//! repo's tasks" into "the rows this frame renders": it applies the
//! visibility filters (status/priority/search/show-completed/show-archived),
//! then orders them either via the manual sort keys (`compare_tasks`, ported
//! unchanged from the pre-split view) or, for the default `Triage` key, via
//! `switchbard_core::triage_rank` — the pure cross-repo ranking function.

use super::{scoped_repos, RepoRow, Snapshot, TaskRow};
use crate::app::HiveApp;
use crate::runtime::{BacklogTaskKey, BacklogTaskSortDirection, BacklogTaskSortKey};
use std::cmp::Ordering;
use std::collections::{BTreeMap, HashMap};
use switchbard_core::{
    parse_backlog_datetime_unix, triage_entry_from_task, triage_rank, BacklogRepo, BacklogTask,
    BacklogTaskSource, BACKLOG_PRIORITIES, CANONICAL_STATUS_ORDER,
};

/// Filter + order every visible task across the current scope. The single
/// entry point `list::render_task_list`, `toolbar`'s visible-count label, and
/// `mod::ensure_selection` all share, so "what's currently on screen" only
/// has one definition.
pub(super) fn visible_task_rows<'a>(app: &HiveApp, snap: &'a Snapshot) -> Vec<TaskRow<'a>> {
    let filter_lc = app.filter().to_lowercase();
    let mut rows: Vec<TaskRow<'a>> = Vec::new();
    for repo in scoped_repos(app, snap) {
        for task in &repo.repo.tasks {
            if task_visible(task, app, &filter_lc) {
                rows.push(TaskRow { repo, task });
            }
        }
    }

    match app.backlog_view.sort_key {
        BacklogTaskSortKey::Triage => sort_by_triage(app, &mut rows),
        sort_key => {
            let direction = app.backlog_view.sort_direction;
            rows.sort_by(|a, b| compare_tasks(a.task, b.task, sort_key, direction));
        }
    }
    rows
}

/// Rank via the pure core function, then re-attach each ranked entry back to
/// its `TaskRow` by `(project_key, task_id)`. `triage_rank` only sees plain
/// data (no lifetimes), which is what keeps it a pure, easily-tested core fn.
fn sort_by_triage<'a>(app: &HiveApp, rows: &mut Vec<TaskRow<'a>>) {
    let overlay = app.ordering_snapshot().overlay;
    let entries: Vec<_> = rows
        .iter()
        .map(|row| {
            triage_entry_from_task(
                row.repo.key.clone(),
                &row.repo.repo_name,
                row.task,
                &row.repo.repo,
            )
        })
        .collect();
    let ranked = triage_rank(&entries, &overlay);

    let mut by_key: HashMap<BacklogTaskKey, TaskRow<'a>> = std::mem::take(rows)
        .into_iter()
        .map(|row| (row.key(), row))
        .collect();
    let mut ordered: Vec<TaskRow<'a>> = ranked
        .into_iter()
        .filter_map(|entry| by_key.remove(&(entry.project_key, entry.task_id)))
        .collect();
    if app.backlog_view.sort_direction == BacklogTaskSortDirection::Descending {
        ordered.reverse();
    }
    *rows = ordered;
}

pub(super) fn compare_tasks(
    a: &BacklogTask,
    b: &BacklogTask,
    sort_key: BacklogTaskSortKey,
    sort_direction: BacklogTaskSortDirection,
) -> Ordering {
    let primary = match sort_key {
        // Triage sorts via `sort_by_triage` above; this comparator is never
        // invoked with this key. Kept in the match (rather than a wildcard)
        // so a future sort key can't silently fall through unhandled.
        BacklogTaskSortKey::Triage => Ordering::Equal,
        BacklogTaskSortKey::Task => cmp_ascii_case_insensitive(&a.id, &b.id)
            .then_with(|| cmp_ascii_case_insensitive(&a.title, &b.title)),
        BacklogTaskSortKey::Status => status_rank(&a.status)
            .cmp(&status_rank(&b.status))
            .then_with(|| cmp_ascii_case_insensitive(&a.status, &b.status)),
        BacklogTaskSortKey::Priority => priority_rank(&a.priority)
            .cmp(&priority_rank(&b.priority))
            .then_with(|| cmp_ascii_case_insensitive(&a.priority, &b.priority)),
        BacklogTaskSortKey::AcceptanceCriteria => acceptance_progress(a)
            .cmp(&acceptance_progress(b))
            .then_with(|| {
                a.acceptance_criteria
                    .len()
                    .cmp(&b.acceptance_criteria.len())
            }),
        // Comma-joined, same string a reader sees on the row/card (list.rs's
        // detail pane, board.rs's strip) — an unlabeled/unassigned/
        // unmilestoned task joins to "", which sorts first ascending.
        BacklogTaskSortKey::Labels => {
            cmp_ascii_case_insensitive(&a.labels.join(", "), &b.labels.join(", "))
        }
        BacklogTaskSortKey::Assignee => {
            cmp_ascii_case_insensitive(&a.assignees.join(", "), &b.assignees.join(", "))
        }
        BacklogTaskSortKey::Project => cmp_ascii_case_insensitive(
            a.project.as_deref().unwrap_or(""),
            b.project.as_deref().unwrap_or(""),
        ),
    };
    let primary = match sort_direction {
        BacklogTaskSortDirection::Ascending => primary,
        BacklogTaskSortDirection::Descending => primary.reverse(),
    };
    primary
        .then_with(|| cmp_ascii_case_insensitive(&a.id, &b.id))
        .then_with(|| cmp_ascii_case_insensitive(&a.title, &b.title))
}

/// Owner UX pass (2026-08-05): sorts by the same shared canonical order
/// every other status surface uses (`ordered_status_vocabulary`'s
/// `CANONICAL_STATUS_ORDER`), not the old 3-entry `BACKLOG_STATUSES` — a
/// task in a repo-specific status like "Icebox" or "In Review" now sorts
/// into its correct kanban position instead of falling to the end.
fn status_rank(status: &str) -> usize {
    CANONICAL_STATUS_ORDER
        .iter()
        .position(|option| option.eq_ignore_ascii_case(status))
        .unwrap_or(CANONICAL_STATUS_ORDER.len())
}

fn priority_rank(priority: &str) -> usize {
    BACKLOG_PRIORITIES
        .iter()
        .position(|option| option.eq_ignore_ascii_case(priority))
        .unwrap_or(BACKLOG_PRIORITIES.len())
}

fn acceptance_progress(task: &BacklogTask) -> usize {
    let total = task.acceptance_criteria.len();
    if total == 0 {
        return 0;
    }
    task.acceptance_done_count() * 1_000 / total
}

pub(super) fn cmp_ascii_case_insensitive(a: &str, b: &str) -> Ordering {
    let mut a = a.bytes();
    let mut b = b.bytes();
    loop {
        match (a.next(), b.next()) {
            (Some(left), Some(right)) => {
                let ordering = left.to_ascii_lowercase().cmp(&right.to_ascii_lowercase());
                if ordering != Ordering::Equal {
                    return ordering;
                }
            }
            (Some(_), None) => return Ordering::Greater,
            (None, Some(_)) => return Ordering::Less,
            (None, None) => return Ordering::Equal,
        }
    }
}

/// One filter control in the toolbar's filter group.
///
/// Used to build a control's option list from the tasks that pass every
/// *other* active filter — see [`ActiveFilters::matches`].
/// Wall clock as unix seconds. `0` if the system clock predates the epoch,
/// which makes every task look freshly touched rather than universally stale
/// — failing toward "do not sweep".
fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Seconds in a day, for the staleness threshold.
const SECONDS_PER_DAY: u64 = 86_400;

/// Whether a task has gone untouched for at least `stale_after_days`.
///
/// Uses the same date the card already shows — `updated_date`, falling back
/// to `created_date` — so "6mo ago" on a card and "stale" in the filter can
/// never disagree. A task whose dates are missing or unparseable is **not**
/// stale: the filter gates a bulk archive, and "I could not read its date"
/// must never mean "safe to sweep away".
pub(super) fn task_is_stale(task: &BacklogTask, now_unix: u64, stale_after_days: u32) -> bool {
    let Some(touched) = task
        .updated_date
        .as_deref()
        .or(task.created_date.as_deref())
        .and_then(parse_backlog_datetime_unix)
    else {
        return false;
    };
    now_unix.saturating_sub(touched) >= u64::from(stale_after_days) * SECONDS_PER_DAY
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Facet {
    Project,
    Label,
}

/// The toolbar's filter group as plain data.
///
/// Lifted out of `HiveApp` so the matching rule is a pure function of its
/// inputs: the option builders below need to evaluate it with one facet
/// suppressed, and the whole thing is otherwise only reachable by
/// constructing a live app.
pub(super) struct ActiveFilters<'a> {
    pub show_completed: bool,
    pub show_archived: bool,
    pub show_drafts: bool,
    pub status: &'a str,
    pub priority: &'a str,
    pub project: &'a str,
    pub label: &'a str,
    pub text_lc: &'a str,
    /// Whether the staleness filter is on, and the threshold + clock it uses.
    /// The clock is captured once per frame rather than read per task so a
    /// batch cannot straddle a second boundary and classify two identical
    /// tasks differently.
    pub stale_only: bool,
    pub stale_after_days: u32,
    pub now_unix: u64,
}

impl<'a> ActiveFilters<'a> {
    pub fn from_app(app: &'a HiveApp, text_lc: &'a str) -> Self {
        Self {
            show_completed: app.backlog_view.show_completed,
            show_archived: app.backlog_view.show_archived,
            show_drafts: app.backlog_view.show_drafts,
            status: &app.backlog_view.status_filter,
            priority: &app.backlog_view.priority_filter,
            project: &app.backlog_view.project_filter,
            label: &app.backlog_view.label_filter,
            text_lc,
            stale_only: app.backlog_view.stale_only,
            stale_after_days: app.config.ui.stale_after_days,
            now_unix: now_unix(),
        }
    }

    /// Whether `task` survives the group, optionally ignoring one facet's own
    /// filter.
    ///
    /// The group is an AND chain, so a control offering values drawn from the
    /// whole repo scope can offer one that yields nothing once the *other*
    /// filters apply — the user picks it and the board empties. Building each
    /// control's options with its own facet excluded (and only its own —
    /// excluding more would over-offer again) is what makes the group behave
    /// as a single filter rather than five independent ones.
    pub fn matches(&self, task: &BacklogTask, exclude: Option<Facet>) -> bool {
        if task_is_completed(task) && !self.show_completed {
            return false;
        }
        if task.source == BacklogTaskSource::Archived && !self.show_archived {
            return false;
        }
        if task.source == BacklogTaskSource::Draft && !self.show_drafts {
            return false;
        }
        if self.status != "all" && !task.status.eq_ignore_ascii_case(self.status) {
            return false;
        }
        if self.priority != "all" && !task.priority.eq_ignore_ascii_case(self.priority) {
            return false;
        }
        if exclude != Some(Facet::Project)
            && self.project != "all"
            && task.project.as_deref() != Some(self.project)
        {
            return false;
        }
        if exclude != Some(Facet::Label)
            && self.label != "all"
            && !task
                .labels
                .iter()
                .any(|label| label.eq_ignore_ascii_case(self.label))
        {
            return false;
        }
        if self.stale_only && !task_is_stale(task, self.now_unix, self.stale_after_days) {
            return false;
        }
        if self.text_lc.is_empty() {
            return true;
        }
        let haystack = [
            task.id.as_str(),
            task.title.as_str(),
            task.status.as_str(),
            task.priority.as_str(),
            task.description.as_str(),
            &task.labels.join(" "),
            &task.assignees.join(" "),
        ]
        .join(" ")
        .to_lowercase();
        haystack.contains(self.text_lc)
    }
}

pub(super) fn task_visible(task: &BacklogTask, app: &HiveApp, filter_lc: &str) -> bool {
    ActiveFilters::from_app(app, filter_lc).matches(task, None)
}

pub(super) fn open_task_count(repo: &BacklogRepo) -> usize {
    repo.tasks
        .iter()
        .filter(|task| !task_is_completed(task) && task.source != BacklogTaskSource::Archived)
        .count()
}

/// Delegates to `BacklogTask::is_done` (core) so the GUI and
/// `backlog_stats`'s burndown/statistics walk share one definition of
/// "done" rather than maintaining two.
pub(super) fn task_is_completed(task: &BacklogTask) -> bool {
    task.is_done()
}

/// One option in a filter control: the value and how many tasks would remain
/// if it were selected.
pub(super) struct FacetOption {
    pub value: String,
    pub count: usize,
}

/// Project names worth offering, alphabetical, each with the number of
/// tasks that would survive selecting it.
///
/// Counted against tasks passing every *other* active filter, so a value is
/// only offered when it actually leads somewhere. `current` is always kept
/// even at zero — dropping the selected value would silently mutate the
/// control the user is looking at, and they need a way back. Def-declared
/// names join at zero too: a project created before any task is assigned
/// must be selectable/assignable, or `project create` would be invisible
/// here.
pub(super) fn project_options(
    scoped: &[&RepoRow],
    filters: &ActiveFilters<'_>,
    current: &str,
) -> Vec<FacetOption> {
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for repo in scoped {
        for task in &repo.repo.tasks {
            if !filters.matches(task, Some(Facet::Project)) {
                continue;
            }
            if let Some(project) = &task.project {
                *counts.entry(project.clone()).or_default() += 1;
            }
        }
        for def in &repo.repo.project_defs {
            counts.entry(def.name.clone()).or_default();
        }
    }
    if current != "all" {
        counts.entry(current.to_string()).or_insert(0);
    }
    counts
        .into_iter()
        .map(|(value, count)| FacetOption { value, count })
        .collect()
}

/// Label values worth offering — see [`project_options`] for why these are
/// counted with their own facet excluded.
pub(super) fn label_options(
    scoped: &[&RepoRow],
    filters: &ActiveFilters<'_>,
    current: &str,
) -> Vec<FacetOption> {
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for repo in scoped {
        for task in &repo.repo.tasks {
            if !filters.matches(task, Some(Facet::Label)) {
                continue;
            }
            for label in &task.labels {
                *counts.entry(label.clone()).or_default() += 1;
            }
        }
    }
    if current != "all" {
        counts.entry(current.to_string()).or_insert(0);
    }
    counts
        .into_iter()
        .map(|(value, count)| FacetOption { value, count })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn task_with_status(status: &str, source: BacklogTaskSource) -> BacklogTask {
        let mut task = task_with_fields(
            &format!("TASK-{}", status.replace(' ', "-")),
            status,
            status,
            "medium",
            0,
            0,
        );
        task.source = source;
        task
    }

    fn task_with_fields(
        id: &str,
        title: &str,
        status: &str,
        priority: &str,
        checked_criteria: usize,
        total_criteria: usize,
    ) -> BacklogTask {
        BacklogTask {
            id: id.to_string(),
            title: title.to_string(),
            status: status.to_string(),
            priority: priority.to_string(),
            assignees: vec![],
            labels: vec![],
            dependencies: vec![],
            references: vec![],
            project: None,
            parent: None,
            created_date: None,
            updated_date: None,
            description: String::new(),
            implementation_plan: String::new(),
            implementation_notes: String::new(),
            final_summary: String::new(),
            acceptance_criteria: (0..total_criteria)
                .map(|index| switchbard_core::BacklogChecklistItem {
                    index: index + 1,
                    checked: index < checked_criteria,
                    text: format!("Criterion {}", index + 1),
                })
                .collect(),
            definition_of_done: vec![],
            source: BacklogTaskSource::Active,
            path: PathBuf::from("/tmp/repo/backlog/tasks/task.md"),
        }
    }

    fn all_filters<'a>() -> ActiveFilters<'a> {
        ActiveFilters {
            show_completed: true,
            show_archived: true,
            show_drafts: true,
            status: "all",
            priority: "all",
            project: "all",
            label: "all",
            text_lc: "",
            stale_only: false,
            stale_after_days: 90,
            now_unix: 0,
        }
    }

    fn project_row(tasks: Vec<BacklogTask>) -> RepoRow {
        RepoRow {
            key: PathBuf::from("/tmp/fixture"),
            repo_name: "fixture".to_string(),
            worktree_label: "main".to_string(),
            branch: Some("main".to_string()),
            repo: BacklogRepo {
                root: PathBuf::from("/tmp/fixture"),
                tasks,
                warnings: vec![],
                project_defs: vec![],
                initiative_defs: vec![],
                goals: vec![],
                ranking: switchbard_core::RepoRanking::default(),
                loaded_at_unix: 0,
                configured_statuses: vec![],
            },
        }
    }

    fn milestone_task(id: &str, status: &str, milestone: &str) -> BacklogTask {
        let mut task = task_with_fields(id, id, status, "medium", 0, 0);
        task.project = Some(milestone.to_string());
        task
    }

    const DAY: u64 = 86_400;

    fn dated(id: &str, updated: Option<&str>, created: Option<&str>) -> BacklogTask {
        let mut t = task_with_fields(id, id, "To Do", "medium", 0, 0);
        t.updated_date = updated.map(str::to_string);
        t.created_date = created.map(str::to_string);
        t
    }

    /// Uses the same date the card shows, so "6mo ago" and "stale" cannot
    /// disagree: `updated_date` wins, `created_date` is the fallback.
    #[test]
    fn staleness_prefers_updated_date_and_falls_back_to_created() {
        let now = 400 * DAY;
        let touched_recently = dated("TASK-1", Some("2026-01-01 00:00"), Some("2020-01-01 00:00"));
        assert!(
            !task_is_stale(
                &touched_recently,
                parse_backlog_datetime_unix("2026-01-10 00:00").unwrap(),
                90
            ),
            "an old task edited recently is not stale"
        );
        let never_updated = dated("TASK-2", None, Some("1970-01-01 00:00"));
        assert!(
            task_is_stale(&never_updated, now, 90),
            "with no updated_date the created_date decides"
        );
    }

    /// The threshold is what makes this configurable, so it must actually
    /// move the boundary rather than being decorative.
    #[test]
    fn the_threshold_moves_the_staleness_boundary() {
        let created = parse_backlog_datetime_unix("2026-01-01 00:00").unwrap();
        let task = dated("TASK-1", Some("2026-01-01 00:00"), None);
        let now = created + 100 * DAY;
        assert!(
            task_is_stale(&task, now, 90),
            "100 days is past a 90-day threshold"
        );
        assert!(
            !task_is_stale(&task, now, 180),
            "and short of a 180-day one"
        );
    }

    /// A task whose dates cannot be read is NOT stale.
    ///
    /// This predicate gates a bulk archive; "I could not parse its date" must
    /// never resolve to "safe to sweep away".
    #[test]
    fn an_undateable_task_is_never_stale() {
        assert!(!task_is_stale(
            &dated("TASK-1", None, None),
            10_000 * DAY,
            1
        ));
        assert!(!task_is_stale(
            &dated("TASK-2", Some("not a date"), Some("also not a date")),
            10_000 * DAY,
            1
        ));
    }

    /// The staleness filter composes with the rest of the group like any
    /// other facet rather than replacing it.
    #[test]
    fn the_staleness_filter_ands_with_the_other_filters() {
        let created = parse_backlog_datetime_unix("2026-01-01 00:00").unwrap();
        let mut stale_high = dated("TASK-1", Some("2026-01-01 00:00"), None);
        stale_high.priority = "high".to_string();
        let mut stale_low = dated("TASK-2", Some("2026-01-01 00:00"), None);
        stale_low.priority = "low".to_string();

        let mut filters = all_filters();
        filters.stale_only = true;
        filters.stale_after_days = 90;
        filters.now_unix = created + 200 * DAY;
        filters.priority = "high";

        assert!(filters.matches(&stale_high, None));
        assert!(
            !filters.matches(&stale_low, None),
            "stale but wrong priority must still be filtered out"
        );
    }

    /// A control must not offer a value that leads nowhere.
    ///
    /// The group is an AND chain, but each control used to draw its options
    /// from the whole repo scope, ignoring the other filters. With
    /// Status=In Progress active, the Milestone picker still listed `v2` —
    /// whose only task is To Do — and choosing it emptied the board.
    #[test]
    fn project_options_exclude_values_no_other_filter_would_leave() {
        let row = project_row(vec![
            milestone_task("TASK-1", "In Progress", "v1"),
            milestone_task("TASK-2", "To Do", "v2"),
        ]);
        let mut filters = all_filters();
        filters.status = "In Progress";

        let options = project_options(&[&row], &filters, "all");
        let values: Vec<&str> = options.iter().map(|o| o.value.as_str()).collect();

        assert_eq!(values, vec!["v1"], "v2 has no In Progress task to offer");
        assert_eq!(options[0].count, 1, "the count is what selecting it yields");
    }

    /// The selected value survives even when the other filters reduce it to
    /// zero: dropping it would mutate the control under the user and leave no
    /// way back to a wider view.
    #[test]
    fn project_options_keep_the_current_selection_at_zero_matches() {
        let row = project_row(vec![
            milestone_task("TASK-1", "In Progress", "v1"),
            milestone_task("TASK-2", "To Do", "v2"),
        ]);
        let mut filters = all_filters();
        filters.status = "In Progress";
        filters.project = "v2";

        let options = project_options(&[&row], &filters, "v2");
        let v2 = options
            .iter()
            .find(|o| o.value == "v2")
            .expect("the active selection must stay listed");
        assert_eq!(v2.count, 0, "and be shown honestly as empty");
    }

    /// A facet never constrains its own option list — otherwise selecting one
    /// label would hide every other label and strand the user on it.
    #[test]
    fn label_options_ignore_the_label_filter_itself() {
        let mut a = task_with_fields("TASK-1", "a", "To Do", "medium", 0, 0);
        a.labels = vec!["frontend".to_string()];
        let mut b = task_with_fields("TASK-2", "b", "To Do", "medium", 0, 0);
        b.labels = vec!["backend".to_string()];
        let row = project_row(vec![a, b]);

        let mut filters = all_filters();
        filters.label = "frontend";

        let values: Vec<String> = label_options(&[&row], &filters, "frontend")
            .into_iter()
            .map(|o| o.value)
            .collect();
        assert_eq!(values, vec!["backend", "frontend"], "both stay switchable");
    }

    /// The other facets still apply to a label list.
    #[test]
    fn label_options_respect_a_status_filter() {
        let mut a = task_with_fields("TASK-1", "a", "In Progress", "medium", 0, 0);
        a.labels = vec!["frontend".to_string()];
        let mut b = task_with_fields("TASK-2", "b", "To Do", "medium", 0, 0);
        b.labels = vec!["backend".to_string()];
        let row = project_row(vec![a, b]);

        let mut filters = all_filters();
        filters.status = "In Progress";

        let values: Vec<String> = label_options(&[&row], &filters, "all")
            .into_iter()
            .map(|o| o.value)
            .collect();
        assert_eq!(values, vec!["frontend"]);
    }

    #[test]
    fn done_status_counts_as_completed_even_before_cleanup_moves_file() {
        let task = task_with_status("Done", BacklogTaskSource::Active);

        assert!(task_is_completed(&task));
    }

    #[test]
    fn open_task_count_excludes_done_and_archived_tasks() {
        let repo = BacklogRepo {
            root: PathBuf::from("/tmp/repo"),
            tasks: vec![
                task_with_status("To Do", BacklogTaskSource::Active),
                task_with_status("In Progress", BacklogTaskSource::Active),
                task_with_status("Done", BacklogTaskSource::Active),
                task_with_status("To Do", BacklogTaskSource::Archived),
            ],
            warnings: vec![],
            project_defs: vec![],
            initiative_defs: vec![],
            goals: vec![],
            ranking: switchbard_core::RepoRanking::default(),
            loaded_at_unix: 0,
            configured_statuses: vec![],
        };

        assert_eq!(open_task_count(&repo), 2);
    }

    #[test]
    fn priority_sort_uses_backlog_priority_order_in_both_directions() {
        let high = task_with_fields("TASK-1", "High priority", "To Do", "high", 0, 0);
        let medium = task_with_fields("TASK-2", "Medium priority", "To Do", "medium", 0, 0);
        let low = task_with_fields("TASK-3", "Low priority", "To Do", "low", 0, 0);
        let mut tasks = [&low, &high, &medium];

        tasks.sort_by(|a, b| {
            compare_tasks(
                a,
                b,
                BacklogTaskSortKey::Priority,
                BacklogTaskSortDirection::Ascending,
            )
        });
        assert_eq!(
            tasks
                .iter()
                .map(|task| task.id.as_str())
                .collect::<Vec<_>>(),
            vec!["TASK-1", "TASK-2", "TASK-3"]
        );

        tasks.sort_by(|a, b| {
            compare_tasks(
                a,
                b,
                BacklogTaskSortKey::Priority,
                BacklogTaskSortDirection::Descending,
            )
        });
        assert_eq!(
            tasks
                .iter()
                .map(|task| task.id.as_str())
                .collect::<Vec<_>>(),
            vec!["TASK-3", "TASK-2", "TASK-1"]
        );
    }

    fn task_with_labels_assignee_milestone(
        id: &str,
        labels: &[&str],
        assignee: Option<&str>,
        milestone: Option<&str>,
    ) -> BacklogTask {
        let mut task = task_with_fields(id, id, "To Do", "medium", 0, 0);
        task.labels = labels.iter().map(|l| l.to_string()).collect();
        task.assignees = assignee.into_iter().map(|a| a.to_string()).collect();
        task.project = milestone.map(|m| m.to_string());
        task
    }

    /// QA parity matrix MEDIUM gap: labels/assignee/milestone sort keys.
    #[test]
    fn labels_sort_orders_by_the_comma_joined_label_string() {
        let none = task_with_labels_assignee_milestone("TASK-1", &[], None, None);
        let alpha = task_with_labels_assignee_milestone("TASK-2", &["alpha"], None, None);
        let zeta = task_with_labels_assignee_milestone("TASK-3", &["zeta"], None, None);
        let mut tasks = [&zeta, &none, &alpha];

        tasks.sort_by(|a, b| {
            compare_tasks(
                a,
                b,
                BacklogTaskSortKey::Labels,
                BacklogTaskSortDirection::Ascending,
            )
        });
        assert_eq!(
            tasks
                .iter()
                .map(|task| task.id.as_str())
                .collect::<Vec<_>>(),
            vec!["TASK-1", "TASK-2", "TASK-3"],
            "unlabeled should sort first ascending (joins to an empty string)"
        );
    }

    #[test]
    fn assignee_sort_is_case_insensitive() {
        let ben = task_with_labels_assignee_milestone("TASK-1", &[], Some("ben"), None);
        let alice = task_with_labels_assignee_milestone("TASK-2", &[], Some("Alice"), None);
        let mut tasks = [&ben, &alice];

        tasks.sort_by(|a, b| {
            compare_tasks(
                a,
                b,
                BacklogTaskSortKey::Assignee,
                BacklogTaskSortDirection::Ascending,
            )
        });
        assert_eq!(
            tasks
                .iter()
                .map(|task| task.id.as_str())
                .collect::<Vec<_>>(),
            vec!["TASK-2", "TASK-1"],
            "\"Alice\" should sort before \"ben\" case-insensitively"
        );
    }

    #[test]
    fn milestone_sort_groups_unmilestoned_tasks_together() {
        let v2 = task_with_labels_assignee_milestone("TASK-1", &[], None, Some("v2"));
        let none = task_with_labels_assignee_milestone("TASK-2", &[], None, None);
        let v1 = task_with_labels_assignee_milestone("TASK-3", &[], None, Some("v1"));
        let mut tasks = [&v2, &none, &v1];

        tasks.sort_by(|a, b| {
            compare_tasks(
                a,
                b,
                BacklogTaskSortKey::Project,
                BacklogTaskSortDirection::Ascending,
            )
        });
        assert_eq!(
            tasks
                .iter()
                .map(|task| task.id.as_str())
                .collect::<Vec<_>>(),
            vec!["TASK-2", "TASK-3", "TASK-1"],
            "unmilestoned (empty string) sorts first, then v1, then v2"
        );
    }

    #[test]
    fn acceptance_sort_orders_by_completion_progress() {
        let empty = task_with_fields("TASK-1", "No AC", "To Do", "medium", 0, 0);
        let partial = task_with_fields("TASK-2", "Partial AC", "To Do", "medium", 1, 3);
        let complete = task_with_fields("TASK-3", "Complete AC", "To Do", "medium", 2, 2);
        let mut tasks = [&complete, &empty, &partial];

        tasks.sort_by(|a, b| {
            compare_tasks(
                a,
                b,
                BacklogTaskSortKey::AcceptanceCriteria,
                BacklogTaskSortDirection::Ascending,
            )
        });
        assert_eq!(
            tasks
                .iter()
                .map(|task| task.id.as_str())
                .collect::<Vec<_>>(),
            vec!["TASK-1", "TASK-2", "TASK-3"]
        );
    }

    #[test]
    fn project_options_include_def_declared_names_at_zero_count() {
        let mut row = project_row(vec![milestone_task("TASK-1", "To Do", "Assigned")]);
        row.repo.project_defs = vec![switchbard_core::ProjectDef {
            name: "Fresh".to_string(),
            status: "Planned".to_string(),
            target_date: None,
            initiative: None,
            lead: None,
            description: String::new(),
            path: PathBuf::from("/tmp/fixture/backlog/projects/Fresh.md"),
        }];

        let filters = all_filters();
        let options = project_options(&[&row], &filters, "all");
        let fresh = options
            .iter()
            .find(|o| o.value == "Fresh")
            .expect("a defined-but-empty project is offered");
        assert_eq!(fresh.count, 0);
        assert!(options
            .iter()
            .any(|o| o.value == "Assigned" && o.count == 1));
    }
}
