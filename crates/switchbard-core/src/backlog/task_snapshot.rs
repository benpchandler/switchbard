//! Authoritative task drafts and optimistic checklist writes for every frontend.
use super::BacklogStorageIdentity;
use anyhow::{ensure, Context, Result};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BacklogTaskSnapshot {
    pub task_id: String,
    pub path: PathBuf,
    pub content: String,
    pub identity: Option<BacklogStorageIdentity>,
}

pub fn read_backlog_task_snapshot(root: &Path, id: &str) -> Result<BacklogTaskSnapshot> {
    let _lock = crate::storage::RepositoryLock::acquire(root)?;
    let task = super::load_backlog_repo(root)?
        .tasks
        .into_iter()
        .find(|task| task.id == id)
        .context("task no longer exists")?;
    Ok(BacklogTaskSnapshot {
        content: super::task_storage::read(&task.path)?,
        task_id: task.id,
        path: task.path,
        identity: task.storage_identity,
    })
}

pub fn validate_backlog_task_snapshot(root: &Path, snapshot: &BacklogTaskSnapshot) -> Result<()> {
    ensure!(
        read_backlog_task_snapshot(root, &snapshot.task_id)? == *snapshot,
        "task changed; reload the draft"
    );
    Ok(())
}

pub fn set_backlog_acceptance_checked_expected(
    root: &Path,
    id: &str,
    index: usize,
    checked: bool,
    snapshot: &BacklogTaskSnapshot,
) -> Result<String> {
    ensure!(snapshot.task_id == id, "draft belongs to a different task");
    let _lock = crate::storage::RepositoryLock::acquire(root)?;
    validate_backlog_task_snapshot(root, snapshot)?;
    let Some(expected) = &snapshot.identity else {
        return super::set_backlog_acceptance_checked(root, id, index, checked);
    };
    let (mut store, repo) = super::task_storage::active(root)?.context("task authority changed")?;
    ensure!(
        repo.0 == expected.repository_id,
        "repository identity changed"
    );
    let locator = snapshot
        .path
        .strip_prefix(root)?
        .to_str()
        .context("invalid task locator")?;
    store.mutate(
        &repo,
        "task",
        locator,
        Some(expected.revision),
        |document| {
            let document = document
                .filter(|doc| !doc.deleted && doc.id == expected.record_id)
                .context("task identity changed")?;
            ensure!(
                document.content == snapshot.content.as_bytes(),
                "task changed; reload draft"
            );
            let (text, _) =
                super::write::edit_text(std::str::from_utf8(&document.content)?, |draft| {
                    super::write::set_task_checklist_item_draft(
                        draft,
                        super::write::TaskChecklist::AcceptanceCriteria,
                        index,
                        checked,
                    )
                })?;
            Ok(Some(text.into_bytes()))
        },
    )?;
    Ok(format!("Edited {id}"))
}

/// Hold the repository fence from draft validation through the native patch write.
pub fn edit_backlog_task_snapshot(
    root: &Path,
    id: &str,
    patch: &super::BacklogTaskPatch,
    snapshot: &BacklogTaskSnapshot,
) -> Result<String> {
    ensure!(snapshot.task_id == id, "draft belongs to a different task");
    let _lock = crate::storage::RepositoryLock::acquire(root)?;
    validate_backlog_task_snapshot(root, snapshot)?;
    super::edit_backlog_task_expected(root, id, patch, snapshot.identity.as_ref())
}
