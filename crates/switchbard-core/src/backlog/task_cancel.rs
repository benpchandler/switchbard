//! Cancellation preserves the task in the native abandonment lifecycle.
use anyhow::{ensure, Context, Result};
use std::path::{Path, PathBuf};

use super::{BacklogTask, BacklogTaskSource};

#[derive(Debug, Clone)]
pub struct PreparedTaskCancellation {
    root: PathBuf,
    task: BacklogTask,
    content: String,
}

impl PreparedTaskCancellation {
    pub fn id(&self) -> &str {
        &self.task.id
    }
    pub fn title(&self) -> &str {
        &self.task.title
    }
}

pub fn prepare_task_cancellation(root: &Path, id: &str) -> Result<PreparedTaskCancellation> {
    let root = root.canonicalize()?;
    let _lock = crate::storage::RepositoryLock::fence(&root, &["task"])?;
    let task = super::load_backlog_repo(&root)?
        .tasks
        .into_iter()
        .find(|task| task.id == id && task.source == BacklogTaskSource::Active)
        .context("active task no longer exists; reload and retry")?;
    ensure!(
        !task.is_done(),
        "Done tasks should be completed, not canceled"
    );
    let content = super::task_storage::read(&task.path)?;
    Ok(PreparedTaskCancellation {
        root,
        task,
        content,
    })
}

/// Revalidate identity, revision and content under the same repository fence as archive.
pub fn cancel_task_expected(root: &Path, prepared: PreparedTaskCancellation) -> Result<String> {
    ensure!(
        root.canonicalize()? == prepared.root,
        "repository changed; reopen confirmation"
    );
    let _lock = crate::storage::RepositoryLock::fence(&prepared.root, &["task"])?;
    let current = prepare_task_cancellation(&prepared.root, prepared.id())?;
    ensure!(
        current.task == prepared.task && current.content == prepared.content,
        "task changed; reload and reopen cancellation"
    );
    if let Some((mut store, repo)) = super::task_storage::active(&prepared.root)? {
        cancel_central(&mut store, &repo, &prepared)?;
    } else {
        super::archive_backlog_task(&prepared.root, prepared.id())?;
    }
    Ok(format!("Canceled {} (record archived)", prepared.id()))
}

fn cancel_central(
    store: &mut crate::storage::Store,
    repo: &crate::storage::RepositoryId,
    prepared: &PreparedTaskCancellation,
) -> Result<()> {
    let expected = prepared
        .task
        .storage_identity
        .as_ref()
        .context("task authority changed")?;
    let destination = format!(
        "backlog/archive/tasks/{}",
        prepared
            .task
            .path
            .file_name()
            .and_then(|name| name.to_str())
            .context("invalid task filename")?
    );
    store.mutate_kind(repo, "task", None, |documents| {
        ensure!(
            !documents.iter().any(|doc| doc.locator == destination),
            "archive destination already exists"
        );
        let document = documents
            .iter_mut()
            .find(|doc| doc.id == expected.record_id && !doc.deleted)
            .context("task no longer exists")?;
        ensure!(
            document.revision == expected.revision
                && document.content == prepared.content.as_bytes(),
            "task changed; reload and reopen cancellation"
        );
        document.ensure_understood()?;
        document.locator = destination;
        Ok(())
    })
}
