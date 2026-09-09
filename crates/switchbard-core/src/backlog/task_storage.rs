//! Task documents retain their raw extensible content. Paths are compatibility
//! locators only; central writes never update the retained migration sources.
use crate::storage::{RepositoryId, Store, MAX_DOCUMENTS};
use anyhow::{ensure, Context, Result};
use std::path::{Path, PathBuf};

pub(super) const TASK_DIRS: [&str; 4] = [
    "backlog/tasks",
    "backlog/completed",
    "backlog/drafts",
    "backlog/archive/tasks",
];
type Sources = Vec<(
    PathBuf,
    Result<String>,
    Option<super::types::BacklogStorageIdentity>,
)>;

pub(super) fn active(root: &Path) -> Result<Option<(Store, RepositoryId)>> {
    let Some(store) = Store::open_existing_default()? else {
        return Ok(None);
    };
    let Some(repo) = store.authority_for_root(root, "task")? else {
        return Ok(None);
    };
    Ok(Some((store, repo)))
}

pub(super) fn root_for_path(path: &Path) -> Option<PathBuf> {
    let dir = path.parent()?;
    TASK_DIRS.iter().find_map(|rel| {
        if !dir.ends_with(rel) {
            return None;
        }
        let mut root = dir;
        for _ in Path::new(rel).components() {
            root = root.parent()?;
        }
        Some(root.to_path_buf())
    })
}

pub(super) fn sources(root: &Path, rel: &str, warnings: &mut Vec<String>) -> Result<Sources> {
    ensure!(TASK_DIRS.contains(&rel), "invalid task directory");
    if let Some((store, repo)) = active(root)? {
        return store
            .list(&repo, "task")?
            .into_iter()
            .filter(|doc| !doc.deleted && Path::new(&doc.locator).parent() == Some(Path::new(rel)))
            .filter_map(|doc| {
                if let Err(error) = doc.ensure_understood() {
                    warnings.push(error.to_string());
                    return None;
                }
                Some(Ok((
                    root.join(doc.locator),
                    String::from_utf8(doc.content).context("stored task is not UTF-8"),
                    Some(super::types::BacklogStorageIdentity {
                        repository_id: doc.repo_id.0,
                        record_id: doc.id,
                        revision: doc.revision,
                    }),
                )))
            })
            .collect();
    }
    let dir = root.join(rel);
    if !dir.is_dir() {
        return Ok(Vec::new());
    }
    let entries = match std::fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(error) => {
            warnings.push(format!("cannot read {}: {error}", dir.display()));
            return Ok(Vec::new());
        }
    };
    let mut paths: Vec<_> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "md"))
        .take(MAX_DOCUMENTS + 1)
        .collect();
    ensure!(
        paths.len() <= MAX_DOCUMENTS,
        "task document count exceeds limit"
    );
    paths.sort();
    Ok(paths
        .into_iter()
        .map(|path| {
            let text = std::fs::read_to_string(&path).map_err(Into::into);
            (path, text, None)
        })
        .collect())
}

pub(super) fn read(path: &Path) -> Result<String> {
    if let Some(root) = root_for_path(path) {
        if let Some((store, repo)) = active(&root)? {
            let locator = path
                .strip_prefix(&root)?
                .to_str()
                .context("non-UTF-8 task locator")?;
            let doc = store
                .read(&repo, "task", locator)?
                .filter(|doc| !doc.deleted)
                .context("task no longer exists")?;
            doc.ensure_understood()?;
            return String::from_utf8(doc.content).context("stored task is not UTF-8");
        }
    }
    std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))
}

pub(super) fn edit<T>(
    path: &Path,
    transform: impl FnOnce(&str) -> Result<(String, T)>,
) -> Result<T> {
    if let Some(root) = root_for_path(path) {
        if let Some((mut store, repo)) = active(&root)? {
            let locator = path
                .strip_prefix(&root)?
                .to_str()
                .context("non-UTF-8 task locator")?;
            let mut result = None;
            store.mutate(&repo, "task", locator, None, |current| {
                let current = current
                    .filter(|doc| !doc.deleted)
                    .context("task no longer exists")?;
                let original =
                    std::str::from_utf8(&current.content).context("stored task is not UTF-8")?;
                let (text, value) = transform(original)?;
                result = Some(value);
                Ok(Some(text.into_bytes()))
            })?;
            return result.context("task mutation did not execute");
        }
    }
    let original =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let (text, result) = transform(&original)?;
    if text != original {
        super::write::atomic_write(path, &text)?;
    }
    Ok(result)
}

pub(super) fn create(path: &Path, text: &str) -> Result<bool> {
    let Some(root) = root_for_path(path) else {
        return Ok(false);
    };
    let Some((mut store, repo)) = active(&root)? else {
        return Ok(false);
    };
    let locator = path
        .strip_prefix(&root)?
        .to_str()
        .context("non-UTF-8 task locator")?;
    let (new_mapping, _) = super::parse::split_frontmatter(text);
    let id_key = serde_yaml::Value::String("id".into());
    let new_id = new_mapping
        .get(&id_key)
        .and_then(serde_yaml::Value::as_str)
        .context("new task has no ID")?;
    store.mutate_kind_with_history(&repo, "task", None, |documents, history| {
        ensure!(
            !history.iter().any(|locator| Path::new(locator)
                .file_stem()
                .and_then(|stem| stem.to_str())
                .and_then(|stem| stem.split_whitespace().next())
                .is_some_and(|id| id.eq_ignore_ascii_case(new_id))),
            "creating task: ID already taken"
        );
        for document in documents.iter() {
            document.ensure_understood()?;
            let (mapping, _) =
                super::parse::split_frontmatter(std::str::from_utf8(&document.content)?);
            ensure!(
                !mapping
                    .get(&id_key)
                    .and_then(serde_yaml::Value::as_str)
                    .is_some_and(|id| id.eq_ignore_ascii_case(new_id)),
                "creating task: ID already taken"
            );
            ensure!(
                document.locator != locator,
                "creating task: locator already taken"
            );
        }
        documents.push(crate::storage::Document {
            content_version: 1,
            id: String::new(),
            repo_id: repo.clone(),
            kind: "task".into(),
            locator: locator.into(),
            revision: 0,
            content: text.as_bytes().to_vec(),
            deleted: false,
        });
        Ok(())
    })?;
    Ok(true)
}

pub(super) fn rehome(
    path: &Path,
    destination: &Path,
    replacement: Option<(&str, &str)>,
) -> Result<bool> {
    let Some(root) = root_for_path(path) else {
        return Ok(false);
    };
    let Some((mut store, repo)) = active(&root)? else {
        return Ok(false);
    };
    ensure!(
        root_for_path(destination).as_ref() == Some(&root),
        "cannot rehome task outside repository"
    );
    let old = path
        .strip_prefix(&root)?
        .to_str()
        .context("non-UTF-8 task locator")?;
    let new = destination
        .strip_prefix(&root)?
        .to_str()
        .context("non-UTF-8 task locator")?;
    store.mutate_kind_with_history(&repo, "task", None, |documents, history| {
        if replacement.is_some() {
            let new_id = destination
                .file_stem()
                .and_then(|stem| stem.to_str())
                .and_then(|stem| stem.split_whitespace().next())
                .context("invalid new task locator")?;
            ensure!(
                !history.iter().any(|locator| Path::new(locator)
                    .file_stem()
                    .and_then(|stem| stem.to_str())
                    .and_then(|stem| stem.split_whitespace().next())
                    .is_some_and(|id| id.eq_ignore_ascii_case(new_id))),
                "new task ID has already been used"
            );
        }
        ensure!(
            !documents.iter().any(|doc| doc.locator == new),
            "destination task already exists"
        );
        let document = documents
            .iter_mut()
            .find(|doc| !doc.deleted && doc.locator == old)
            .context("task no longer exists")?;
        document.ensure_understood()?;
        if let Some((expected, text)) = replacement {
            ensure!(
                document.content == expected.as_bytes(),
                "task changed while planning rehome; reload and retry"
            );
            document.content = text.as_bytes().to_vec();
        } else {
            let text = std::str::from_utf8(&document.content)?;
            let (mapping, _) = super::parse::split_frontmatter(text);
            let done = mapping
                .get(serde_yaml::Value::String("status".into()))
                .and_then(serde_yaml::Value::as_str)
                .is_some_and(|status| status.eq_ignore_ascii_case("done"));
            if new.starts_with("backlog/completed/") {
                ensure!(done, "only a Done task can be completed");
            }
            if new.starts_with("backlog/archive/tasks/") {
                ensure!(!done, "Done tasks should be completed, not archived");
            }
        }
        document.locator = new.to_owned();
        Ok(())
    })?;
    Ok(true)
}

#[cfg(test)]
#[path = "task_storage_tests.rs"]
mod tests;
