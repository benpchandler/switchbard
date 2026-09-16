//! Planning and ordered membership commit together.
use super::{
    planning::{planning_order, PlanningState},
    BacklogRepo, WriteOutcome,
};
use crate::storage::{Document, RepositoryLock};
use anyhow::{ensure, Context, Result};
use std::path::Path;

pub(super) fn set(
    root: &Path,
    id: &str,
    state: PlanningState,
    expected: Option<&super::BacklogTask>,
    expected_content: Option<&str>,
) -> Result<WriteOutcome> {
    let _lock = RepositoryLock::fence(root, &["task", "ranking"])?;
    let central = super::central_commands::command_store(root, &["task", "ranking"])?;
    let sequence = central
        .as_ref()
        .map(|(store, _)| store.change_sequence())
        .transpose()?;
    let repo = super::load_backlog_repo(root)?;
    let path = super::mutations::resolve_task_file(root, id)?;
    let task = repo
        .tasks
        .iter()
        .find(|t| t.path == path)
        .context("task disappeared")?;
    ensure!(
        expected.is_none_or(|expected| expected == task),
        "task changed; reload before changing planning"
    );
    ensure!(task.editable(), "historical task is read-only");
    let original = super::task_storage::read(&path)?;
    ensure!(
        expected_content.is_none_or(|expected| expected == original),
        "task changed; reload before changing planning"
    );
    let (updated, changed) = super::write::edit_text(&original, |draft| {
        super::write::set_task_planning_draft(draft, state)
    })?;
    let ids = order_after(&repo, &task.id, state);
    let ranking_changed = repo.ranking.planned.as_ref() != Some(&ids);
    let ranking_path = root.join("backlog/ranking.yml");
    if let Some((mut store, repository)) = central {
        store.mutate_kinds(
            &repository,
            &["task", "ranking"],
            sequence,
            |documents, _| {
                let locator = path
                    .strip_prefix(root)?
                    .to_str()
                    .context("non-UTF-8 task path")?;
                let document = documents
                    .iter_mut()
                    .find(|d| d.kind == "task" && d.locator == locator && !d.deleted)
                    .context("task disappeared")?;
                ensure!(
                    document.content == original.as_bytes(),
                    "task changed; reload"
                );
                document.content = updated.as_bytes().to_vec();
                let position = documents
                    .iter()
                    .position(|d| d.kind == "ranking" && d.locator == "backlog/ranking.yml");
                let prior = position.and_then(|i| {
                    (!documents[i].deleted).then_some(documents[i].content.as_slice())
                });
                let content = super::ranking::planned_document(prior, &ids)?;
                if let Some(index) = position {
                    documents[index].content = content;
                    documents[index].deleted = false;
                } else {
                    documents.push(Document {
                        id: String::new(),
                        repo_id: repository.clone(),
                        kind: "ranking".into(),
                        locator: "backlog/ranking.yml".into(),
                        revision: 0,
                        content,
                        content_version: 1,
                        deleted: false,
                    });
                }
                Ok(())
            },
        )?;
    } else {
        let prior = match std::fs::read(&ranking_path) {
            Ok(bytes) => Some(bytes),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => return Err(error.into()),
        };
        let ranking = super::ranking::planned_document(prior.as_deref(), &ids)?;
        // Validate and stage both bytes before either write; restore the task on ranking failure.
        super::write::atomic_write(&path, &updated)?;
        let result = if ranking_path.exists() {
            super::write::atomic_write(&ranking_path, std::str::from_utf8(&ranking)?)
        } else {
            let temp = ranking_path.with_extension("planning.tmp");
            std::fs::write(&temp, &ranking)
                .and_then(|()| std::fs::rename(&temp, &ranking_path))
                .map_err(Into::into)
        };
        if let Err(error) = result {
            super::write::atomic_write(&path, &original)
                .context("restoring task after ranking failure")?;
            return Err(error);
        }
    }
    Ok(if changed.changed() || ranking_changed {
        WriteOutcome::Changed
    } else {
        WriteOutcome::Unchanged
    })
}
fn order_after(repo: &BacklogRepo, id: &str, state: PlanningState) -> Vec<String> {
    let mut ids = planning_order(repo);
    let mut updated = repo
        .tasks
        .iter()
        .find(|task| task.id == id)
        .expect("resolved task")
        .clone();
    updated.planning = state;
    if !super::planning::eligible(&updated) {
        ids.retain(|entry| entry != id);
    } else if !ids.iter().any(|entry| entry == id) {
        ids.push(id.to_owned());
    }
    ids
}
