//! Existing-task parent resolution and the one-level sub-issue rule.
//! UI candidates and writes share this authority; loading resolves legacy
//! shorthand in memory without rewriting task documents.

use super::allocate::strip_id_prefix;
use super::parse::{configured_task_prefix, DEFAULT_TASK_PREFIX};
use super::types::{BacklogRepo, BacklogTask, BacklogTaskSource};
use anyhow::{bail, Context, Result};
use std::collections::HashMap;

/// Existing eligible parents, in snapshot order, using the repository's
/// configured prefix so legacy TASK IDs and current IDs resolve identically
/// to writes. Reads config when opening a picker, never on its render path.
/// A blocked source returns its reason rather than an empty list. The current
/// parent remains a candidate, allowing callers to mark it selected.
pub fn eligible_backlog_parents<'a>(
    repo: &'a BacklogRepo,
    task_id: &str,
) -> Result<Vec<&'a BacklogTask>> {
    let prefix = configured_task_prefix(&repo.root)?;
    let task = task_in_repo(&repo.tasks, task_id, &prefix)?;
    validate_move_source(&repo.tasks, task, &prefix)?;
    Ok(repo
        .tasks
        .iter()
        .filter(|parent| validate_parent(parent, Some(task), &prefix).is_ok())
        .collect())
}

pub(super) fn validate_move_source(
    tasks: &[BacklogTask],
    task: &BacklogTask,
    prefix: &str,
) -> Result<()> {
    if task.source != BacklogTaskSource::Active {
        bail!(
            "{} is {} - only active tasks (backlog/tasks) can be moved",
            task.id,
            task.source.label()
        );
    }
    if tasks.iter().any(|other| {
        other
            .parent
            .as_deref()
            .is_some_and(|p| same_id(p, &task.id, prefix))
            || other
                .id
                .rsplit_once('.')
                .is_some_and(|(p, _)| same_id(p, &task.id, prefix))
    }) {
        bail!(
            "{} has sub-issues - move or promote them first (sub-issues nest one level)",
            task.id
        );
    }
    Ok(())
}

pub(super) fn resolve_parent<'a>(
    tasks: &'a [BacklogTask],
    task: Option<&BacklogTask>,
    wanted: &str,
    prefix: &str,
) -> Result<&'a BacklogTask> {
    let parent = task_in_repo(tasks, wanted, prefix)?;
    validate_parent(parent, task, prefix)?;
    Ok(parent)
}

fn validate_parent(parent: &BacklogTask, task: Option<&BacklogTask>, prefix: &str) -> Result<()> {
    if task.is_some_and(|task| same_id(&parent.id, &task.id, prefix)) {
        bail!("{} cannot be its own parent", parent.id);
    }
    if parent.parent.is_some() || normalized_id(&parent.id, prefix).contains('.') {
        bail!(
            "{} is itself a sub-issue - sub-issues nest one level, pick a top-level parent",
            parent.id
        );
    }
    normalized_id(&parent.id, prefix)
        .parse::<u32>()
        .with_context(|| format!("cannot allocate a subtask id under parent `{}`", parent.id))?;
    Ok(())
}

pub(super) fn normalize_parent_links(tasks: &mut [BacklogTask], prefix: &str) {
    let ids: HashMap<String, String> = tasks
        .iter()
        .map(|task| {
            (
                normalized_id(&task.id, prefix).to_ascii_lowercase(),
                task.id.clone(),
            )
        })
        .collect();
    for task in tasks {
        if let Some(parent) = task.parent.as_mut() {
            if let Some(id) = ids.get(&normalized_id(parent, prefix).to_ascii_lowercase()) {
                parent.clone_from(id);
            }
        }
    }
}

pub(super) fn task_in_repo<'a>(
    tasks: &'a [BacklogTask],
    wanted: &str,
    prefix: &str,
) -> Result<&'a BacklogTask> {
    tasks
        .iter()
        .find(|task| same_id(&task.id, wanted, prefix))
        .with_context(|| format!("no task {wanted} in this repo"))
}

pub(super) fn same_id(a: &str, b: &str, prefix: &str) -> bool {
    normalized_id(a, prefix).eq_ignore_ascii_case(normalized_id(b, prefix))
}

pub(super) fn normalized_id<'a>(task_id: &'a str, prefix: &str) -> &'a str {
    let trimmed = task_id.trim();
    strip_id_prefix(trimmed, prefix)
        .or_else(|| strip_id_prefix(trimmed, DEFAULT_TASK_PREFIX))
        .unwrap_or(trimmed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backlog::{
        create_task_allocating_id, load_backlog_repo, move_backlog_task, NewBacklogTask,
    };
    use std::fs;

    fn fixture(prefix: &str) -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("fixture");
        fs::create_dir_all(dir.path().join("backlog/tasks")).expect("tasks");
        fs::write(
            dir.path().join("backlog/config.yml"),
            format!("task_prefix: {prefix}\n"),
        )
        .expect("config");
        for (id, parent) in [
            ("7", ""),
            ("7.1", "parent_task_id: '7'\n"),
            ("8", ""),
            ("9", "parent_task_id: '404'\n"),
        ] {
            fs::write(
                dir.path()
                    .join(format!("backlog/tasks/{}-{id}.md", prefix.to_lowercase())),
                format!("---\nid: {prefix}-{id}\ntitle: Title {id}\n{parent}---\n"),
            )
            .expect("task");
        }
        dir
    }

    #[test]
    fn loaded_links_use_existing_identity_without_rewriting_files() {
        for prefix in ["TASK", "LED"] {
            let dir = fixture(prefix);
            let path = dir
                .path()
                .join(format!("backlog/tasks/{}-7.1.md", prefix.to_lowercase()));
            let before = fs::read_to_string(&path).expect("before");
            let repo = load_backlog_repo(dir.path()).expect("load");
            let child = task_in_repo(&repo.tasks, "7.1", prefix).expect("child");
            assert_eq!(
                child.parent.as_deref(),
                Some(format!("{prefix}-7").as_str())
            );
            assert_eq!(
                task_in_repo(&repo.tasks, "9", prefix)
                    .expect("orphan")
                    .parent
                    .as_deref(),
                Some("404")
            );
            assert_eq!(fs::read_to_string(path).expect("after"), before);
        }
    }

    #[test]
    fn candidates_and_move_share_existing_task_eligibility() {
        let dir = fixture("LED");
        let repo = load_backlog_repo(dir.path()).expect("load");
        let candidates = eligible_backlog_parents(&repo, "LED-7.1").expect("candidates");
        assert_eq!(
            candidates
                .iter()
                .map(|task| task.id.as_str())
                .collect::<Vec<_>>(),
            ["LED-7", "LED-8"]
        );
        assert!(eligible_backlog_parents(&repo, "LED-7")
            .expect_err("has children")
            .to_string()
            .contains("has sub-issues"));
        for parent in ["7.1", "9", "404"] {
            assert!(move_backlog_task(dir.path(), "7.1", Some(parent)).is_err());
        }
        assert_eq!(
            move_backlog_task(dir.path(), "7.1", Some("7")).expect("current"),
            None
        );
        assert_eq!(
            move_backlog_task(dir.path(), "7.1", Some("8"))
                .expect("move")
                .as_deref(),
            Some("LED-8.1")
        );
    }

    #[test]
    fn configured_prefix_keeps_legacy_source_candidates_in_agreement_with_moves() {
        let dir = fixture("LED");
        let path = dir.path().join("backlog/tasks/led-8.md");
        fs::write(&path, "---\nid: TASK-8\ntitle: Legacy source\n---\n").expect("legacy task");
        let repo = load_backlog_repo(dir.path()).expect("load");
        let candidates = eligible_backlog_parents(&repo, "TASK-8").expect("candidates");
        assert_eq!(
            candidates
                .iter()
                .map(|task| task.id.as_str())
                .collect::<Vec<_>>(),
            ["LED-7"]
        );
        assert_eq!(
            move_backlog_task(dir.path(), "TASK-8", Some("LED-7"))
                .expect("move")
                .as_deref(),
            Some("LED-7.2")
        );
        let reloaded = load_backlog_repo(dir.path()).expect("reload");
        assert_eq!(
            task_in_repo(&reloaded.tasks, "7.2", "LED")
                .expect("moved")
                .parent
                .as_deref(),
            Some("LED-7")
        );
    }

    #[test]
    fn moving_to_legacy_parent_stores_its_actual_identity() {
        let dir = fixture("LED");
        fs::write(
            dir.path().join("backlog/tasks/led-7.md"),
            "---\nid: TASK-7\ntitle: Legacy parent\n---\n",
        )
        .expect("legacy parent");
        let repo = load_backlog_repo(dir.path()).expect("load");
        let candidates = eligible_backlog_parents(&repo, "LED-8").expect("candidates");
        assert_eq!(
            candidates
                .iter()
                .map(|task| task.id.as_str())
                .collect::<Vec<_>>(),
            ["TASK-7"]
        );
        let id = move_backlog_task(dir.path(), "LED-8", Some("TASK-7"))
            .expect("move")
            .expect("new id");
        assert_eq!(id, "LED-7.2");
        let repo = load_backlog_repo(dir.path()).expect("reload");
        let child = task_in_repo(&repo.tasks, &id, "LED").expect("child");
        let text = fs::read_to_string(&child.path).expect("task file");
        assert!(text.contains("parent_task_id: TASK-7\n"), "{text}");
        assert_eq!(child.parent.as_deref(), Some("TASK-7"));
        assert_eq!(
            move_backlog_task(dir.path(), &id, Some("7")).expect("same parent"),
            None
        );
        assert_eq!(fs::read_to_string(&child.path).expect("no-op file"), text);
    }

    #[test]
    fn stale_candidates_are_revalidated_before_mutation() {
        let dir = fixture("TASK");
        let repo = load_backlog_repo(dir.path()).expect("load");
        assert!(eligible_backlog_parents(&repo, "TASK-7.1")
            .expect("candidates")
            .iter()
            .any(|t| t.id == "TASK-8"));
        fs::remove_file(dir.path().join("backlog/tasks/task-8.md"))
            .expect("delete selected parent");
        assert!(move_backlog_task(dir.path(), "7.1", Some("8"))
            .expect_err("stale")
            .to_string()
            .contains("no task"));
        assert!(dir.path().join("backlog/tasks/task-7.1.md").exists());
    }

    #[test]
    fn creation_resolves_shorthand_and_rejects_missing_or_nested_parent() {
        let dir = fixture("LED");
        let mut task = NewBacklogTask {
            title: "New child".into(),
            description: String::new(),
            status: "To Do".into(),
            priority: String::new(),
            acceptance_criteria: Vec::new(),
            parent: Some("8".into()),
            labels: Vec::new(),
            assignees: Vec::new(),
            project: None,
            dependencies: Vec::new(),
            custom: Vec::new(),
        };
        let (_, path) = create_task_allocating_id(dir.path(), &task).expect("create shorthand");
        assert!(fs::read_to_string(path)
            .expect("created")
            .contains("parent_task_id: LED-8\n"));
        for parent in ["404", "7.1", "9"] {
            task.parent = Some(parent.into());
            assert!(create_task_allocating_id(dir.path(), &task).is_err());
        }
    }

    #[test]
    fn completed_source_and_decimal_children_without_links_block_moving() {
        let dir = fixture("TASK");
        let mut repo = load_backlog_repo(dir.path()).expect("load");
        let child = repo
            .tasks
            .iter_mut()
            .find(|t| t.id == "TASK-7.1")
            .expect("child");
        child.parent = None;
        assert!(eligible_backlog_parents(&repo, "TASK-7").is_err());
        repo.tasks
            .iter_mut()
            .find(|t| t.id == "TASK-8")
            .expect("source")
            .source = BacklogTaskSource::Completed;
        assert!(eligible_backlog_parents(&repo, "TASK-8")
            .expect_err("completed")
            .to_string()
            .contains("only active tasks"));
    }
}
