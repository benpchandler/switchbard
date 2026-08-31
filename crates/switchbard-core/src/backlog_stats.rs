//! Cross-repo Backlog statistics + burndown — the pure domain logic behind
//! TASK-16's Statistics lens.
//!
//! Everything here is derived from `BacklogTask` metadata already loaded by
//! `load_backlog_repo` (status, priority, `created_date`, `updated_date`,
//! `milestone`). No new store, no schema change, no IO — the GUI passes in
//! the same `BacklogRepo`s it already caches for the list/board lenses.
//!
//! ## Burndown is a completion trend, not a due-date burndown
//!
//! Backlog.md v1.47 has no due-date field (see `backlog_triage`'s module
//! doc for the same constraint applied to triage). So "burndown" here means:
//! for each day since the oldest parseable `created_date` in scope, how many
//! in-scope tasks existed (created on/before that day) versus how many of
//! those were already done (their `updated_date` — the best available proxy
//! for "last touched", which for a task sitting in Done is its completion
//! edit — falls on/before that day). Tasks with an unparseable or missing
//! `created_date` are excluded from the timeline; they still count in the
//! snapshot totals in [`CrossRepoStats`].

use crate::backlog::{parse_backlog_day as parse_day, BacklogRepo, BacklogTask};
use crate::backlog_relations::is_blocked;
use std::collections::BTreeMap;

/// Loop bound for the burndown day-by-day walk (Power-of-10 rule 2: every
/// loop over derived/external input needs a fixed upper bound). ~11 years —
/// far beyond any real task's age — so it only ever fires on a corrupt clock,
/// never on legitimate data.
const MAX_BURNDOWN_DAYS: i64 = 4_000;

/// One repo's slice of the cross-repo snapshot — also the row shape for the
/// task-19 Portfolio lens (per-repo health).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoStats {
    pub repo: String,
    pub total: usize,
    pub done: usize,
    pub by_status: BTreeMap<String, usize>,
    pub by_priority: BTreeMap<String, usize>,
    /// How many in-scope, not-done tasks are currently blocked
    /// (`backlog_relations::is_blocked`) — task-19's "blocked count".
    pub blocked: usize,
    /// `created_date` of the oldest not-done task, as Backlog stores it
    /// (`"YYYY-MM-DD HH:MM"`) — task-19's "oldest open age". `None` when
    /// every not-done task's `created_date` is missing/unparseable, or there
    /// are no open tasks.
    pub oldest_open_created_date: Option<String>,
    /// The most recent `updated_date` across every in-scope task — task-19's
    /// "last activity". `None` when nothing in scope has a parseable date.
    pub last_activity_updated_date: Option<String>,
}

impl RepoStats {
    pub fn completion_pct(&self) -> f64 {
        if self.total == 0 {
            return 0.0;
        }
        (self.done as f64 / self.total as f64) * 100.0
    }

    pub fn open(&self) -> usize {
        self.total - self.done
    }
}

/// The full cross-repo snapshot: totals plus a per-repo breakdown.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CrossRepoStats {
    pub total_tasks: usize,
    pub done_tasks: usize,
    pub by_status: BTreeMap<String, usize>,
    pub by_priority: BTreeMap<String, usize>,
    pub per_repo: Vec<RepoStats>,
}

impl CrossRepoStats {
    pub fn completion_pct(&self) -> f64 {
        if self.total_tasks == 0 {
            return 0.0;
        }
        (self.done_tasks as f64 / self.total_tasks as f64) * 100.0
    }
}

/// Compute totals, completion %, and status/priority distributions across
/// every tracked project, with a per-repo breakdown. Archived tasks are
/// excluded — they've explicitly opted out of the active count the same way
/// they're excluded from `sort::open_task_count`.
pub fn compute_cross_repo_stats(projects: &[(String, &BacklogRepo)]) -> CrossRepoStats {
    assert!(
        projects.iter().all(|(name, _)| !name.is_empty()),
        "invariant: every repo passed to compute_cross_repo_stats has a name"
    );
    let mut total_tasks = 0usize;
    let mut done_tasks = 0usize;
    let mut by_status = BTreeMap::new();
    let mut by_priority = BTreeMap::new();
    let mut per_repo = Vec::with_capacity(projects.len());

    for (repo, project) in projects {
        let mut repo_total = 0usize;
        let mut repo_done = 0usize;
        let mut repo_blocked = 0usize;
        let mut repo_status = BTreeMap::new();
        let mut repo_priority = BTreeMap::new();
        let mut oldest_open: Option<(i64, &str)> = None;
        let mut last_activity: Option<(i64, &str)> = None;
        for task in in_scope_tasks(project) {
            repo_total += 1;
            *repo_status.entry(task.status.clone()).or_insert(0usize) += 1;
            *repo_priority.entry(task.priority.clone()).or_insert(0usize) += 1;
            let done = task.is_done();
            if done {
                repo_done += 1;
            } else if is_blocked(task, project) {
                repo_blocked += 1;
            }
            if !done {
                if let Some(created) = task.created_date.as_deref() {
                    if let Some(day) = parse_day(created) {
                        if oldest_open.is_none_or(|(best, _)| day < best) {
                            oldest_open = Some((day, created));
                        }
                    }
                }
            }
            if let Some(updated) = task.updated_date.as_deref() {
                if let Some(day) = parse_day(updated) {
                    if last_activity.is_none_or(|(best, _)| day > best) {
                        last_activity = Some((day, updated));
                    }
                }
            }
        }
        for (status, count) in &repo_status {
            *by_status.entry(status.clone()).or_insert(0usize) += count;
        }
        for (priority, count) in &repo_priority {
            *by_priority.entry(priority.clone()).or_insert(0usize) += count;
        }
        total_tasks += repo_total;
        done_tasks += repo_done;
        per_repo.push(RepoStats {
            repo: repo.clone(),
            total: repo_total,
            done: repo_done,
            by_status: repo_status,
            by_priority: repo_priority,
            blocked: repo_blocked,
            oldest_open_created_date: oldest_open.map(|(_, s)| s.to_string()),
            last_activity_updated_date: last_activity.map(|(_, s)| s.to_string()),
        });
    }

    per_repo.sort_by(|a, b| a.repo.cmp(&b.repo));
    assert!(
        done_tasks <= total_tasks,
        "invariant: done tasks can't exceed total tasks"
    );
    CrossRepoStats {
        total_tasks,
        done_tasks,
        by_status,
        by_priority,
        per_repo,
    }
}

/// Archived tasks are excluded from every statistic; everything else
/// (active, draft, completed) counts. Kept as its own function so the two
/// call sites (the cross-repo snapshot and the burndown walk) can't drift.
fn in_scope_tasks(project: &BacklogRepo) -> impl Iterator<Item = &BacklogTask> {
    project
        .tasks
        .iter()
        .filter(|task| task.source != crate::backlog::BacklogTaskSource::Archived)
}

/// One day's point on a burndown series.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BurndownPoint {
    /// Days since the Unix epoch (`chrono::NaiveDate::num_days_from_ce`-style
    /// arithmetic isn't needed — epoch days are enough for plotting and
    /// keeps this module `chrono`-internals-free at the type level).
    pub day: i64,
    pub completed_cumulative: usize,
    pub remaining: usize,
}

/// A named burndown series — "Overall" or one per milestone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BurndownSeries {
    pub label: String,
    pub points: Vec<BurndownPoint>,
}

/// Build the overall completion-trend burndown across every in-scope task
/// with a parseable `created_date`. See the module doc for what "burndown"
/// means without a due-date field.
pub fn compute_burndown(tasks: &[&BacklogTask], today_unix_day: i64) -> BurndownSeries {
    BurndownSeries {
        label: "Overall".to_string(),
        points: burndown_points(tasks, today_unix_day),
    }
}

/// Same as [`compute_burndown`], grouped by `BacklogTask::project`. Tasks
/// with no project are omitted — a per-project view has nothing
/// meaningful to say about them.
pub fn compute_burndown_by_project(
    tasks: &[&BacklogTask],
    today_unix_day: i64,
) -> Vec<BurndownSeries> {
    let mut by_project: BTreeMap<String, Vec<&BacklogTask>> = BTreeMap::new();
    for task in tasks {
        if let Some(project) = &task.project {
            by_project.entry(project.clone()).or_default().push(task);
        }
    }
    by_project
        .into_iter()
        .map(|(label, tasks)| BurndownSeries {
            points: burndown_points(&tasks, today_unix_day),
            label,
        })
        .collect()
}

/// One project's roll-up: member-task counts joined (by exact name) to its
/// optional definition. Computed, never stored — the same posture as every
/// other statistic in this module.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectRollup {
    pub name: String,
    pub done: usize,
    pub total: usize,
    /// From the def file when one exists; `None` for a reference-only
    /// project (tasks name it, nothing defines it yet).
    pub status: Option<String>,
    pub target_date: Option<String>,
    pub lead: Option<String>,
    pub initiative: Option<String>,
    pub has_def: bool,
}

impl ProjectRollup {
    pub fn completion_pct(&self) -> f64 {
        if self.total == 0 {
            return 0.0;
        }
        (self.done as f64 / self.total as f64) * 100.0
    }
}

/// One initiative's roll-up: the sum of its member projects, joined to its
/// optional definition. `name: None` is the "no initiative" bucket —
/// projects that don't name one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InitiativeRollup {
    pub name: Option<String>,
    pub done: usize,
    pub total: usize,
    pub status: Option<String>,
    pub target_date: Option<String>,
    pub has_def: bool,
    /// Member projects, name-sorted.
    pub projects: Vec<ProjectRollup>,
}

impl InitiativeRollup {
    pub fn completion_pct(&self) -> f64 {
        if self.total == 0 {
            return 0.0;
        }
        (self.done as f64 / self.total as f64) * 100.0
    }
}

/// The full Initiative → Project roll-up across every passed repo, plus the
/// bucket of tasks with no project at all.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HierarchyRollup {
    /// Name-sorted, with the `name: None` "no initiative" bucket last (when
    /// present). An initiative appears if a project references it or a def
    /// declares it.
    pub initiatives: Vec<InitiativeRollup>,
    pub unassigned_done: usize,
    pub unassigned_total: usize,
}

/// Compute the Initiative → Project → task roll-up (trajectory:
/// *Linear-vocabulary hierarchy*, divergence 4).
///
/// A project **exists** if any in-scope task references it OR a def file
/// declares it — a defined-but-empty project rolls up as `0/0`, which is
/// what makes `project create` before task assignment meaningful. Same rule
/// one tier up for initiatives. Membership joins are exact string matches,
/// cross-repo. When two repos define the same name, the first definition in
/// input order wins (callers pass repos in a stable, name-sorted order), so
/// the result is deterministic; archived tasks are excluded, as everywhere
/// in this module.
pub fn compute_hierarchy_rollup(repos: &[&BacklogRepo]) -> HierarchyRollup {
    use crate::backlog::{InitiativeDef, ProjectDef};

    // First definition in input order wins on name conflicts.
    let mut project_defs: BTreeMap<&str, &ProjectDef> = BTreeMap::new();
    let mut initiative_defs: BTreeMap<&str, &InitiativeDef> = BTreeMap::new();
    for repo in repos {
        for def in &repo.project_defs {
            project_defs.entry(def.name.as_str()).or_insert(def);
        }
        for def in &repo.initiative_defs {
            initiative_defs.entry(def.name.as_str()).or_insert(def);
        }
    }

    let mut counts: BTreeMap<&str, (usize, usize)> = BTreeMap::new();
    let mut unassigned_done = 0usize;
    let mut unassigned_total = 0usize;
    for repo in repos {
        for task in in_scope_tasks(repo) {
            let done = usize::from(task.is_done());
            match task.project.as_deref() {
                Some(project) => {
                    let entry = counts.entry(project).or_insert((0, 0));
                    entry.0 += done;
                    entry.1 += 1;
                }
                None => {
                    unassigned_done += done;
                    unassigned_total += 1;
                }
            }
        }
    }

    // Union of referenced and defined names, BTreeMap-ordered (= name-sorted).
    let mut project_names: BTreeMap<&str, ()> = BTreeMap::new();
    project_names.extend(counts.keys().map(|name| (*name, ())));
    project_names.extend(project_defs.keys().map(|name| (*name, ())));

    let mut by_initiative: BTreeMap<Option<&str>, Vec<ProjectRollup>> = BTreeMap::new();
    for (name, ()) in project_names {
        let (done, total) = counts.get(name).copied().unwrap_or((0, 0));
        let def = project_defs.get(name).copied();
        let rollup = ProjectRollup {
            name: name.to_string(),
            done,
            total,
            status: def.map(|d| d.status.clone()),
            target_date: def.and_then(|d| d.target_date.clone()),
            lead: def.and_then(|d| d.lead.clone()),
            initiative: def.and_then(|d| d.initiative.clone()),
            has_def: def.is_some(),
        };
        by_initiative
            .entry(def.and_then(|d| d.initiative.as_deref()))
            .or_default()
            .push(rollup);
    }

    // An initiative that is defined but has no member projects still appears.
    for name in initiative_defs.keys() {
        by_initiative.entry(Some(name)).or_default();
    }

    // `Option`'s ordering puts `None` first; the "no initiative" bucket
    // belongs last, so collect the named ones then move the bucket back.
    let mut initiatives: Vec<InitiativeRollup> = Vec::with_capacity(by_initiative.len());
    let mut no_initiative: Option<InitiativeRollup> = None;
    for (name, projects) in by_initiative {
        let def = name.and_then(|n| initiative_defs.get(n).copied());
        let rollup = InitiativeRollup {
            name: name.map(str::to_string),
            done: projects.iter().map(|p| p.done).sum(),
            total: projects.iter().map(|p| p.total).sum(),
            status: def.map(|d| d.status.clone()),
            target_date: def.and_then(|d| d.target_date.clone()),
            has_def: def.is_some(),
            projects,
        };
        if rollup.name.is_none() {
            no_initiative = Some(rollup);
        } else {
            initiatives.push(rollup);
        }
    }
    initiatives.extend(no_initiative);

    HierarchyRollup {
        initiatives,
        unassigned_done,
        unassigned_total,
    }
}

/// Shared walk behind both burndown entry points: bucket tasks by their
/// parsed created/completed day, then sweep forward accumulating totals.
fn burndown_points(tasks: &[&BacklogTask], today_unix_day: i64) -> Vec<BurndownPoint> {
    let mut created_on: BTreeMap<i64, usize> = BTreeMap::new();
    let mut completed_on: BTreeMap<i64, usize> = BTreeMap::new();
    let mut earliest = today_unix_day;

    for task in tasks {
        let Some(created_day) = task.created_date.as_deref().and_then(parse_day) else {
            continue;
        };
        *created_on.entry(created_day).or_insert(0) += 1;
        earliest = earliest.min(created_day);

        if task.is_done() {
            let completed_day = task
                .updated_date
                .as_deref()
                .and_then(parse_day)
                .filter(|day| *day >= created_day)
                .unwrap_or(created_day);
            *completed_on.entry(completed_day).or_insert(0) += 1;
        }
    }

    if created_on.is_empty() {
        return Vec::new();
    }
    let span_days = (today_unix_day - earliest).clamp(0, MAX_BURNDOWN_DAYS);
    assert!(span_days >= 0, "invariant: burndown span is never negative");
    assert!(
        span_days <= MAX_BURNDOWN_DAYS,
        "invariant: burndown span respects its fixed upper bound"
    );

    let mut points = Vec::with_capacity((span_days + 1) as usize);
    let mut created_cumulative = 0usize;
    let mut completed_cumulative = 0usize;
    for offset in 0..=span_days {
        let day = earliest + offset;
        created_cumulative += created_on.get(&day).copied().unwrap_or(0);
        completed_cumulative += completed_on.get(&day).copied().unwrap_or(0);
        points.push(BurndownPoint {
            day,
            completed_cumulative,
            remaining: created_cumulative.saturating_sub(completed_cumulative),
        });
    }
    points
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backlog::{BacklogChecklistItem, BacklogTaskSource};
    use std::path::PathBuf;

    fn task(id: &str, status: &str, priority: &str, source: BacklogTaskSource) -> BacklogTask {
        BacklogTask {
            id: id.to_string(),
            title: id.to_string(),
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
            acceptance_criteria: Vec::<BacklogChecklistItem>::new(),
            definition_of_done: vec![],
            source,
            path: PathBuf::from("/repo/backlog/tasks/task.md"),
        }
    }

    fn project(root: &str, tasks: Vec<BacklogTask>) -> BacklogRepo {
        BacklogRepo {
            root: PathBuf::from(root),
            tasks,
            warnings: vec![],
            project_defs: vec![],
            initiative_defs: vec![],
            goals: vec![],
            loaded_at_unix: 0,
            configured_statuses: vec![],
        }
    }

    #[test]
    fn cross_repo_stats_sum_across_repos_and_exclude_archived() {
        let repo_a = project(
            "/a",
            vec![
                task("TASK-1", "Done", "high", BacklogTaskSource::Active),
                task("TASK-2", "To Do", "medium", BacklogTaskSource::Active),
                task("TASK-3", "To Do", "low", BacklogTaskSource::Archived),
            ],
        );
        let repo_b = project(
            "/b",
            vec![task("TASK-1", "Done", "high", BacklogTaskSource::Active)],
        );

        let stats =
            compute_cross_repo_stats(&[("a".to_string(), &repo_a), ("b".to_string(), &repo_b)]);

        assert_eq!(stats.total_tasks, 3);
        assert_eq!(stats.done_tasks, 2);
        assert_eq!(stats.by_status.get("Done"), Some(&2));
        assert_eq!(stats.by_priority.get("high"), Some(&2));
        assert_eq!(stats.per_repo.len(), 2);
        assert!((stats.completion_pct() - 200.0 / 3.0).abs() < 1e-9);
    }

    #[test]
    fn repo_stats_report_blocked_count_oldest_open_and_last_activity() {
        let mut old_open = task("TASK-1", "To Do", "high", BacklogTaskSource::Active);
        old_open.created_date = Some("2026-01-01 00:00".to_string());
        old_open.updated_date = Some("2026-01-01 00:00".to_string());

        let mut newer_open_and_blocked =
            task("TASK-2", "To Do", "medium", BacklogTaskSource::Active);
        newer_open_and_blocked.created_date = Some("2026-02-01 00:00".to_string());
        newer_open_and_blocked.updated_date = Some("2026-03-01 00:00".to_string());
        newer_open_and_blocked.dependencies = vec!["TASK-1".to_string()];

        let repo = project("/a", vec![old_open, newer_open_and_blocked]);
        let stats = compute_cross_repo_stats(&[("a".to_string(), &repo)]);

        let repo_stats = &stats.per_repo[0];
        assert_eq!(repo_stats.blocked, 1, "TASK-2 depends on open TASK-1");
        assert_eq!(repo_stats.open(), 2);
        assert_eq!(
            repo_stats.oldest_open_created_date.as_deref(),
            Some("2026-01-01 00:00")
        );
        assert_eq!(
            repo_stats.last_activity_updated_date.as_deref(),
            Some("2026-03-01 00:00")
        );
    }

    #[test]
    fn empty_snapshot_reports_zero_percent_not_nan() {
        let stats = compute_cross_repo_stats(&[]);
        assert_eq!(stats.completion_pct(), 0.0);
    }

    #[test]
    fn burndown_accumulates_created_and_completed_by_day() {
        let mut created_only = task("TASK-1", "To Do", "high", BacklogTaskSource::Active);
        created_only.created_date = Some("2026-01-01 00:00".to_string());

        let mut completed = task("TASK-2", "Done", "high", BacklogTaskSource::Active);
        completed.created_date = Some("2026-01-01 00:00".to_string());
        completed.updated_date = Some("2026-01-03 00:00".to_string());

        let today = parse_day("2026-01-05 00:00").unwrap();
        let series = compute_burndown(&[&created_only, &completed], today);

        assert_eq!(series.label, "Overall");
        let first = series.points.first().unwrap();
        assert_eq!(first.remaining, 2);
        assert_eq!(first.completed_cumulative, 0);
        let last = series.points.last().unwrap();
        assert_eq!(last.completed_cumulative, 1);
        assert_eq!(last.remaining, 1);
    }

    #[test]
    fn tasks_without_a_parseable_created_date_are_excluded_from_the_timeline() {
        let task = task("TASK-1", "Done", "high", BacklogTaskSource::Active);
        let series = compute_burndown(&[&task], 100);
        assert!(series.points.is_empty());
    }

    #[test]
    fn burndown_by_milestone_groups_and_skips_unassigned_tasks() {
        let mut with_milestone = task("TASK-1", "Done", "high", BacklogTaskSource::Active);
        with_milestone.created_date = Some("2026-01-01 00:00".to_string());
        with_milestone.updated_date = Some("2026-01-01 00:00".to_string());
        with_milestone.project = Some("v1".to_string());

        let mut unassigned = task("TASK-2", "To Do", "high", BacklogTaskSource::Active);
        unassigned.created_date = Some("2026-01-01 00:00".to_string());

        let today = parse_day("2026-01-02 00:00").unwrap();
        let series = compute_burndown_by_project(&[&with_milestone, &unassigned], today);

        assert_eq!(series.len(), 1);
        assert_eq!(series[0].label, "v1");
    }

    fn project_def(name: &str, status: &str, initiative: Option<&str>) -> crate::ProjectDef {
        crate::ProjectDef {
            name: name.to_string(),
            status: status.to_string(),
            target_date: None,
            initiative: initiative.map(str::to_string),
            lead: None,
            description: String::new(),
            path: PathBuf::from(format!("/repo/backlog/projects/{name}.md")),
        }
    }

    fn initiative_def(name: &str, status: &str) -> crate::InitiativeDef {
        crate::InitiativeDef {
            name: name.to_string(),
            status: status.to_string(),
            target_date: None,
            description: String::new(),
            path: PathBuf::from(format!("/repo/backlog/initiatives/{name}.md")),
        }
    }

    fn task_in(project_name: &str, id: &str, status: &str) -> BacklogTask {
        let mut t = task(id, status, "medium", BacklogTaskSource::Active);
        t.project = Some(project_name.to_string());
        t
    }

    #[test]
    fn hierarchy_rollup_unions_referenced_and_defined_and_buckets_by_initiative() {
        let mut repo_a = project(
            "/a",
            vec![
                task_in("Alpha", "TASK-1", "Done"),
                task_in("Alpha", "TASK-2", "To Do"),
                task("TASK-3", "To Do", "low", BacklogTaskSource::Active),
            ],
        );
        repo_a.project_defs = vec![
            project_def("Alpha", "In Progress", Some("Big")),
            project_def("Empty", "Planned", None),
        ];
        repo_a.initiative_defs = vec![initiative_def("Big", "In Progress")];
        let repo_b = project("/b", vec![task_in("Beta", "TASK-1", "Done")]);

        let rollup = compute_hierarchy_rollup(&[&repo_a, &repo_b]);

        assert_eq!(rollup.unassigned_total, 1);
        assert_eq!(rollup.unassigned_done, 0);

        // "Big" first (named, sorted), the no-initiative bucket last.
        assert_eq!(rollup.initiatives.len(), 2);
        let big = &rollup.initiatives[0];
        assert_eq!(big.name.as_deref(), Some("Big"));
        assert!(big.has_def);
        assert_eq!(big.status.as_deref(), Some("In Progress"));
        assert_eq!((big.done, big.total), (1, 2), "sums its member projects");
        assert_eq!(big.projects.len(), 1);
        assert_eq!(big.projects[0].name, "Alpha");
        assert!(big.projects[0].has_def);
        assert!((big.projects[0].completion_pct() - 50.0).abs() < 1e-9);

        let bucket = &rollup.initiatives[1];
        assert_eq!(bucket.name, None, "no-initiative bucket renders last");
        assert!(!bucket.has_def);
        let names: Vec<&str> = bucket.projects.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(
            names,
            vec!["Beta", "Empty"],
            "name-sorted within the bucket"
        );
        let beta = &bucket.projects[0];
        assert!(!beta.has_def, "reference-only project still appears");
        assert_eq!(beta.status, None);
        assert_eq!((beta.done, beta.total), (1, 1));
        let empty = &bucket.projects[1];
        assert!(empty.has_def);
        assert_eq!(
            (empty.done, empty.total),
            (0, 0),
            "defined-but-empty rolls up as 0/0"
        );
    }

    #[test]
    fn hierarchy_rollup_excludes_archived_and_resolves_def_conflicts_first_wins() {
        let mut repo_a = project("/a", vec![task_in("Dup", "TASK-1", "To Do")]);
        repo_a.project_defs = vec![project_def("Dup", "Planned", None)];
        let mut repo_b = project(
            "/b",
            vec![{
                let mut t = task_in("Dup", "TASK-9", "Done");
                t.source = BacklogTaskSource::Archived;
                t
            }],
        );
        repo_b.project_defs = vec![project_def("Dup", "Completed", None)];

        let rollup = compute_hierarchy_rollup(&[&repo_a, &repo_b]);
        let dup = &rollup.initiatives[0].projects[0];
        assert_eq!(dup.name, "Dup");
        assert_eq!(
            dup.status.as_deref(),
            Some("Planned"),
            "first definition in input order wins"
        );
        assert_eq!(
            (dup.done, dup.total),
            (0, 1),
            "the archived task is excluded"
        );
    }

    #[test]
    fn hierarchy_rollup_shows_a_defined_but_projectless_initiative() {
        let mut repo = project("/a", vec![]);
        repo.initiative_defs = vec![initiative_def("Lonely", "Planned")];

        let rollup = compute_hierarchy_rollup(&[&repo]);
        assert_eq!(rollup.initiatives.len(), 1);
        assert_eq!(rollup.initiatives[0].name.as_deref(), Some("Lonely"));
        assert!(rollup.initiatives[0].projects.is_empty());
        assert_eq!(
            (rollup.initiatives[0].done, rollup.initiatives[0].total),
            (0, 0)
        );
    }
}
