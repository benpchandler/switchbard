//! Planning intent is independent of execution status and checklist coverage.
use super::{BacklogRepo, BacklogTaskSource, RankPlacement, WriteOutcome};
use anyhow::{bail, Result};
use std::{collections::HashSet, fmt, path::Path, str::FromStr};

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub enum PlanningState {
    Considering,
    Planned,
}
impl PlanningState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Considering => "Considering",
            Self::Planned => "Planned",
        }
    }
    pub fn legacy(status: &str, source: BacklogTaskSource) -> Self {
        if source == BacklogTaskSource::Draft
            || ["icebox", "backlog", "draft"]
                .iter()
                .any(|s| status.eq_ignore_ascii_case(s))
        {
            Self::Considering
        } else {
            Self::Planned
        }
    }
    pub fn for_new(status: &str) -> Self {
        if ["in progress", "waiting", "in review", "done"]
            .iter()
            .any(|s| status.eq_ignore_ascii_case(s))
        {
            Self::Planned
        } else {
            Self::Considering
        }
    }
}
impl fmt::Display for PlanningState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}
impl FromStr for PlanningState {
    type Err = anyhow::Error;
    fn from_str(value: &str) -> Result<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "considering" => Ok(Self::Considering),
            "planned" => Ok(Self::Planned),
            _ => bail!("unknown planning value {value:?}; expected Considering or Planned"),
        }
    }
}
pub(super) fn eligible(task: &super::BacklogTask) -> bool {
    task.planning == PlanningState::Planned
        && task.source == BacklogTaskSource::Active
        && !task.is_done()
        && !["canceled", "cancelled", "archived"]
            .iter()
            .any(|s| task.status.eq_ignore_ascii_case(s))
}
/// Ordered planned work, with legacy order as the deterministic seed/fallback.
pub fn planning_order(repo: &BacklogRepo) -> Vec<String> {
    let mut eligible_ids = std::collections::HashMap::new();
    for task in repo.tasks.iter().filter(|t| eligible(t)) {
        eligible_ids
            .entry(task.id.to_ascii_lowercase())
            .or_insert(task.id.as_str());
    }
    let mut seen = HashSet::new();
    repo.ranking
        .planned
        .iter()
        .flatten()
        .map(String::as_str)
        .chain(repo.tasks.iter().map(|t| t.id.as_str()))
        .filter_map(|id| eligible_ids.get(&id.to_ascii_lowercase()).copied())
        .filter(|id| seen.insert(id.to_ascii_lowercase()))
        .map(str::to_owned)
        .collect()
}
pub fn set_task_planning(root: &Path, id: &str, state: PlanningState) -> Result<WriteOutcome> {
    super::planning_write::set(root, id, state, None, None)
}
pub fn rank_planned_task(root: &Path, id: &str, placement: &RankPlacement) -> Result<WriteOutcome> {
    super::ranking::rank_planned(root, id, placement)
}

pub fn set_task_planning_expected(
    root: &Path,
    id: &str,
    state: PlanningState,
    expected: &super::BacklogTask,
) -> Result<WriteOutcome> {
    super::planning_write::set(root, id, state, Some(expected), None)
}

/// Change planning while the complete captured document is still current.
pub fn set_task_planning_snapshot(
    root: &Path,
    id: &str,
    state: PlanningState,
    snapshot: &super::BacklogTaskSnapshot,
) -> Result<WriteOutcome> {
    let _lock = crate::storage::RepositoryLock::fence(root, &["task", "ranking"])?;
    anyhow::ensure!(snapshot.task_id == id, "task selection changed; reload");
    super::validate_backlog_task_snapshot(root, snapshot)?;
    // Without a held fence the snapshot can go stale after validation; the
    // write rechecks the captured content inside its own transaction.
    super::planning_write::set(root, id, state, None, Some(&snapshot.content))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn repo() -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir_all(dir.path().join("backlog/tasks")).expect("mkdir");
        std::fs::write(
            dir.path().join("backlog/config.yml"),
            "statuses: [To Do, In Progress, Done]\n",
        )
        .expect("config");
        dir
    }
    fn task(root: &Path, id: &str, extra: &str, body: &str) {
        std::fs::write(
            root.join(format!("backlog/tasks/{id}.md")),
            format!("---\nid: {id}\ntitle: {id}\nstatus: To Do\n{extra}---\n{body}"),
        )
        .expect("task");
    }
    #[test]
    fn planning_is_independent_and_promotions_append_and_legacy_scopes_survive() {
        let dir = repo();
        let root = dir.path();
        task(root, "TASK-1", "planning: Considering\n", "");
        task(root, "TASK-2", "planning: Considering\n", "");
        assert!(set_task_planning(root, "TASK-2", PlanningState::Planned)
            .expect("plan")
            .changed());
        assert!(set_task_planning(root, "TASK-1", PlanningState::Planned)
            .expect("plan")
            .changed());
        let loaded = super::super::load_backlog_repo(root).expect("load");
        assert_eq!(planning_order(&loaded), ["TASK-2", "TASK-1"]);
        assert!(loaded.tasks.iter().all(|task| task.status == "To Do"));
        assert!(rank_planned_task(root, "TASK-1", &RankPlacement::Top)
            .expect("rank")
            .changed());
        assert_eq!(
            planning_order(&super::super::load_backlog_repo(root).expect("load")),
            ["TASK-1", "TASK-2"]
        );
        assert!(
            set_task_planning(root, "TASK-1", PlanningState::Considering)
                .expect("consider")
                .changed()
        );
        assert!(rank_planned_task(root, "TASK-1", &RankPlacement::Top).is_err());
    }
    #[test]
    fn rollup_counts_descendants_once_excludes_canceled_and_does_not_complete() {
        let dir = repo();
        let root = dir.path();
        task(root, "TASK-1", "", "");
        let four = "## Acceptance Criteria\n- [ ] one\n- [ ] two\n- [ ] three\n- [ ] four\n";
        task(root, "TASK-1.1", "", &four.replacen("[ ]", "[x]", 1));
        task(root, "TASK-1.2", "", four);
        let mut loaded = super::super::load_backlog_repo(root).expect("load");
        let p = super::super::checklist_progress(&loaded);
        assert_eq!(p["TASK-1"].percentage(), Some(12.5));
        assert_eq!(p["TASK-1"].total, 8);
        loaded
            .tasks
            .iter_mut()
            .find(|t| t.id == "TASK-1.2")
            .expect("child")
            .status = "Canceled".into();
        loaded
            .tasks
            .iter_mut()
            .find(|t| t.id == "TASK-1")
            .expect("parent")
            .parent = Some("TASK-1.1".into());
        let p = super::super::checklist_progress(&loaded);
        assert_eq!(p["TASK-1"].total, 4);
        assert_eq!(p["TASK-1"].checked, 1);
        assert!(loaded.tasks.iter().all(|t| !t.is_done()));
    }
    #[test]
    fn unknown_planning_is_not_silently_inferred_and_empty_is_unmeasured() {
        assert!("Committed".parse::<PlanningState>().is_err());
        assert_eq!(
            super::super::ChecklistProgress::default().percentage(),
            None
        );
        assert_eq!(PlanningState::for_new("To Do"), PlanningState::Considering);
        assert_eq!(
            PlanningState::legacy("To Do", BacklogTaskSource::Active),
            PlanningState::Planned
        );
    }
    #[test]
    fn stale_snapshot_refuses_planning_change() {
        let dir = repo();
        let root = dir.path();
        task(root, "TASK-1", "", "old");
        let snapshot = super::super::read_backlog_task_snapshot(root, "TASK-1").expect("snapshot");
        task(root, "TASK-1", "", "new");
        assert!(
            set_task_planning_snapshot(root, "TASK-1", PlanningState::Considering, &snapshot)
                .is_err()
        );
    }
}

#[cfg(test)]
mod central_tests {
    use super::*;
    use crate::storage::{with_test_database, MigrationPlan, SourceDocument, Store};
    #[test]
    fn central_planning_preserves_identity_and_updates_order_in_one_transaction() {
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path().join("repo");
        let db = temp.path().join("store.sqlite");
        std::fs::create_dir_all(root.join("backlog/tasks")).expect("mkdir");
        let raw="---\nid: TASK-1\ntitle: Test\nstatus: Not started\nplanning: Considering\n---\n## Acceptance Criteria\n- [ ] Keep unchecked\n";
        std::fs::write(root.join("backlog/tasks/task-1.md"), raw).expect("task");
        std::fs::write(
            root.join("backlog/config.yml"),
            "statuses: [Not started, In Progress, Done]\n",
        )
        .expect("config");
        let mut store = Store::open(&db).expect("store");
        let repo_id = store.bind_repository(&root).expect("bind");
        let plan = MigrationPlan::capture(
            repo_id.clone(),
            vec!["task".into(), "ranking".into()],
            vec![SourceDocument {
                kind: "task".into(),
                locator: "backlog/tasks/task-1.md".into(),
                path: root.join("backlog/tasks/task-1.md"),
            }],
        )
        .expect("capture");
        store.apply_migration(&plan).expect("migrate");
        let before = store.list(&repo_id, "task").expect("list")[0].clone();
        with_test_database(&db, || {
            assert!(set_task_planning(&root, "TASK-1", PlanningState::Planned)
                .expect("plan")
                .changed());
            let loaded = super::super::load_backlog_repo(&root).expect("load");
            assert_eq!(planning_order(&loaded), ["TASK-1"]);
            assert_eq!(loaded.tasks[0].acceptance_done_count(), 0);
            let after = store.list(&repo_id, "task").expect("list")[0].clone();
            assert_eq!(after.id, before.id);
            assert_eq!(after.revision, before.revision + 1);
            assert!(!set_task_planning(&root, "TASK-1", PlanningState::Planned)
                .expect("again")
                .changed());
            let patch = super::super::BacklogTaskPatch {
                status: Some("To Do".into()),
                ..Default::default()
            };
            super::super::edit_backlog_task(&root, "TASK-1", &patch).expect("legacy status alias");
            assert_eq!(
                super::super::load_backlog_repo(&root).expect("load").tasks[0].status,
                "Not started"
            );
            let mut new = super::super::NewBacklogTask {
                title: "Active first".into(),
                description: String::new(),
                status: "In Progress".into(),
                priority: "low".into(),
                acceptance_criteria: vec![],
                parent: None,
                labels: vec![],
                assignees: vec![],
                project: None,
                dependencies: vec![],
                due_date: None,
                custom: vec![],
            };
            let first = super::super::create_backlog_task(&root, &new).expect("first");
            new.title = "Active second".into();
            new.priority = "high".into();
            let second = super::super::create_backlog_task(&root, &new).expect("second");
            let order = vec!["TASK-1".to_owned(), first.clone(), second];
            assert_eq!(
                planning_order(&super::super::load_backlog_repo(&root).expect("load")),
                order
            );
            super::super::edit_backlog_task(
                &root,
                &first,
                &super::super::BacklogTaskPatch {
                    status: Some("To Do".into()),
                    ..Default::default()
                },
            )
            .expect("status");
            assert_eq!(
                planning_order(&super::super::load_backlog_repo(&root).expect("load")),
                order
            );
        });
        assert_eq!(
            std::fs::read_to_string(root.join("backlog/tasks/task-1.md")).expect("read"),
            raw
        );
    }
}
