//! Commands whose invariants span centrally owned document kinds.
//! A mixed strangler stage refuses these commands before any write.
use super::{allocate, goals, mutations, parse, ranking, write, BacklogTaskSource, TaskListField};
use crate::storage::{Document, RepositoryId, Store};
use anyhow::{ensure, Context, Result};
use std::path::Path;

pub(super) fn command_store(root: &Path, kinds: &[&str]) -> Result<Option<(Store, RepositoryId)>> {
    let Some(store) = Store::open_existing_default()? else {
        return Ok(None);
    };
    let Some(repo) = store.repository(root)? else {
        return Ok(None);
    };
    let authority = kinds
        .iter()
        .map(|kind| store.authority(&repo, kind))
        .collect::<Result<Vec<_>>>()?;
    if !authority.iter().any(|active| *active) {
        return Ok(None);
    }
    ensure!(
        authority.iter().all(|active| *active),
        "command spans partially migrated data; migrate all affected kinds ({}) before retrying",
        kinds.join(", ")
    );
    Ok(Some((store, repo)))
}

pub(super) fn move_task(
    root: &Path,
    task_id: &str,
    new_parent: Option<&str>,
) -> Result<Option<Option<String>>> {
    Ok(move_task_with_edit(root, task_id, new_parent, |text| {
        Ok((text.to_owned(), false))
    })?
    .map(|(moved, _)| moved))
}

pub(super) fn move_task_with_edit(
    root: &Path,
    task_id: &str,
    new_parent: Option<&str>,
    edit: impl FnOnce(&str) -> Result<(String, bool)>,
) -> Result<Option<(Option<String>, bool)>> {
    let _repository_lock = crate::storage::RepositoryLock::acquire(root)?;
    let kinds = ["task", "goals", "ranking"];
    let Some((mut store, repo)) = command_store(root, &kinds)? else {
        return Ok(None);
    };
    let prefix = parse::configured_task_prefix(root)?;
    let result = store.mutate_kinds(&repo, &kinds, None, |documents, history| {
        for document in documents.iter() {
            document.ensure_understood()?;
        }
        let mut matches = documents
            .iter()
            .enumerate()
            .filter(|(_, doc)| doc.kind == "task" && !doc.deleted)
            .filter_map(|(index, doc)| {
                let parsed = std::str::from_utf8(&doc.content).ok().and_then(|text| {
                    parse::parse_task_text(
                        &root.join(&doc.locator),
                        task_source(&doc.locator).ok()?,
                        text,
                    )
                    .ok()
                });
                parsed
                    .filter(|(task, _)| mutations::same_id(&task.id, task_id, &prefix))
                    .map(|_| index)
            })
            .collect::<Vec<_>>();
        ensure!(
            matches.len() == 1,
            "task ID must resolve to exactly one central document"
        );
        let document = &mut documents[matches.remove(0)];
        let (text, changed) = edit(std::str::from_utf8(&document.content)?)?;
        document.content = text.into_bytes();
        let tasks = documents
            .iter()
            .filter(|doc| doc.kind == "task" && !doc.deleted)
            .map(|doc| {
                let source = task_source(&doc.locator)?;
                let text = std::str::from_utf8(&doc.content)?;
                Ok(parse::parse_task_text(&root.join(&doc.locator), source, text)?.0)
            })
            .collect::<Result<Vec<_>>>()?;
        let Some((task, parent)) = mutations::move_target(&tasks, task_id, new_parent, &prefix)?
        else {
            return Ok((None, changed));
        };
        let id = allocate::central_candidate(
            history
                .get("task")
                .context("missing task allocation history")?,
            parent.as_deref(),
            &prefix,
        )?;
        let full_id = format!("{prefix}-{id}");
        let old_locator = task
            .path
            .strip_prefix(root)?
            .to_str()
            .context("non-UTF-8 task locator")?;
        let moved = documents
            .iter_mut()
            .find(|doc| doc.kind == "task" && doc.locator == old_locator)
            .context("task disappeared")?;
        let (destination, text) = write::rehome_document(
            &task.path,
            std::str::from_utf8(&moved.content)?,
            &prefix,
            &id,
            parent.as_deref(),
        )?;
        moved.locator = destination
            .strip_prefix(root)?
            .to_str()
            .context("non-UTF-8 task locator")?
            .to_owned();
        moved.content = text.into_bytes();
        for other in &tasks {
            if !other
                .dependencies
                .iter()
                .any(|dep| mutations::same_id(dep, &task.id, &prefix))
            {
                continue;
            }
            let dependencies = other
                .dependencies
                .iter()
                .map(|dep| {
                    if mutations::same_id(dep, &task.id, &prefix) {
                        full_id.clone()
                    } else {
                        dep.clone()
                    }
                })
                .collect::<Vec<_>>();
            let locator = if other.path == task.path {
                destination.as_path()
            } else {
                other.path.as_path()
            }
            .strip_prefix(root)?
            .to_str()
            .context("non-UTF-8 task locator")?;
            let doc = documents
                .iter_mut()
                .find(|doc| doc.kind == "task" && doc.locator == locator)
                .context("dependent task disappeared")?;
            let (text, _) = write::edit_text(std::str::from_utf8(&doc.content)?, |draft| {
                write::set_task_list_field_draft(draft, TaskListField::Dependencies, &dependencies)
            })?;
            doc.content = text.into_bytes();
        }
        rename_aggregate(
            documents,
            "goals",
            &task.id,
            &full_id,
            goals::rename_task_document,
        )?;
        rename_aggregate(
            documents,
            "ranking",
            &task.id,
            &full_id,
            ranking::rename_task_document,
        )?;
        Ok((Some(full_id), changed))
    })?;
    Ok(Some(result))
}

pub(super) fn rename_aggregate(
    documents: &mut [Document],
    kind: &str,
    old: &str,
    new: &str,
    rename: impl FnOnce(Option<&[u8]>, &str, &str) -> Result<Option<Vec<u8>>>,
) -> Result<()> {
    let canonical = match kind {
        "goals" => "backlog/goals.yml",
        "ranking" => "backlog/ranking.yml",
        _ => anyhow::bail!("unsupported aggregate kind"),
    };
    let active: Vec<_> = documents
        .iter()
        .filter(|doc| doc.kind == kind && !doc.deleted)
        .collect();
    ensure!(
        active.len() <= 1 && active.iter().all(|doc| doc.locator == canonical),
        "ambiguous {kind} aggregate; reconcile before renaming"
    );
    let Some(document) = documents
        .iter_mut()
        .find(|doc| doc.kind == kind && !doc.deleted)
    else {
        return Ok(());
    };
    document.ensure_understood()?;
    if let Some(content) = rename(Some(&document.content), old, new)? {
        document.content = content;
    }
    Ok(())
}

fn task_source(locator: &str) -> Result<BacklogTaskSource> {
    match Path::new(locator).parent().and_then(Path::to_str) {
        Some("backlog/tasks") => Ok(BacklogTaskSource::Active),
        Some("backlog/completed") => Ok(BacklogTaskSource::Completed),
        Some("backlog/drafts") => Ok(BacklogTaskSource::Draft),
        Some("backlog/archive/tasks") => Ok(BacklogTaskSource::Archived),
        _ => anyhow::bail!("invalid task lifecycle locator: {locator}"),
    }
}
