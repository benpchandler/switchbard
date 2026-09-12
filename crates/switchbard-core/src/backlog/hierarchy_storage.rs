//! Storage routing for hierarchy definitions. Logical paths remain compatibility
//! handles after cutover; migrated operations never touch their original files.

use super::{
    filename_slug, split_frontmatter, yaml_string, WriteOutcome, INITIATIVES_DIR, PROJECTS_DIR,
};
use crate::storage::{Document, RepositoryId, Store, MAX_DOCUMENTS};
use anyhow::{bail, Context, Result};
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};

type Sources = Vec<(PathBuf, Result<String>)>;

fn kind(rel: &str) -> Result<&'static str> {
    match rel {
        PROJECTS_DIR => Ok("project"),
        INITIATIVES_DIR => Ok("initiative"),
        _ => bail!("unsupported hierarchy locator: {rel}"),
    }
}

pub(super) fn rel_for_path(root: &Path, path: &Path) -> Result<&'static str> {
    let relative = path
        .strip_prefix(root)
        .context("definition outside repository")?;
    for rel in [PROJECTS_DIR, INITIATIVES_DIR] {
        if relative.parent() == Some(Path::new(rel)) {
            return Ok(rel);
        }
    }
    bail!("invalid hierarchy definition locator: {}", path.display())
}

fn active(root: &Path, rel: &str) -> Result<Option<(Store, RepositoryId)>> {
    let Some(store) = Store::open_existing_default()? else {
        return Ok(None);
    };
    let Some(repo) = store.repository(root)? else {
        return Ok(None);
    };
    if !store.authority(&repo, kind(rel)?)? {
        return Ok(None);
    }
    Ok(Some((store, repo)))
}

pub(super) fn is_active(root: &Path, rel: &str) -> Result<bool> {
    Ok(active(root, rel)?.is_some())
}

pub(super) fn sources(root: &Path, rel: &str, warnings: &mut Vec<String>) -> Result<Sources> {
    if let Some((store, repo)) = active(root, rel)? {
        return store
            .list(&repo, kind(rel)?)?
            .into_iter()
            .filter(|doc| !doc.deleted)
            .filter_map(|doc| {
                if let Err(error) = doc.ensure_understood() {
                    warnings.push(error.to_string());
                    return None;
                }
                Some((|| {
                    let path = root.join(&doc.locator);
                    anyhow::ensure!(
                        rel_for_path(root, &path)? == rel,
                        "stored hierarchy kind and locator disagree"
                    );
                    let text =
                        String::from_utf8(doc.content).context("stored definition is not UTF-8")?;
                    Ok((path, Ok(text)))
                })())
            })
            .collect();
    }
    let dir = root.join(rel);
    if !dir.is_dir() {
        return Ok(Vec::new());
    }
    let entries = match fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(_) => {
            warnings.push(format!("cannot read {}", dir.display()));
            return Ok(Vec::new());
        }
    };
    let mut paths: Vec<_> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(OsStr::to_str) == Some("md"))
        .take(MAX_DOCUMENTS + 1)
        .collect();
    anyhow::ensure!(
        paths.len() <= MAX_DOCUMENTS,
        "hierarchy definition count exceeds limit"
    );
    paths.sort();
    Ok(paths
        .into_iter()
        .map(|path| {
            let text = fs::read_to_string(&path).map_err(Into::into);
            (path, text)
        })
        .collect())
}

pub(super) fn create(root: &Path, rel: &str, name: &str, text: &str) -> Result<Option<PathBuf>> {
    let Some((mut store, repo)) = active(root, rel)? else {
        return Ok(None);
    };
    let slug = filename_slug(name);
    let locator = format!("{rel}/{slug}.md");
    store.mutate_checked(
        &repo,
        kind(rel)?,
        &locator,
        None,
        |documents| validate_available_name(documents, name, None),
        |current| {
            if current.is_some_and(|doc| !doc.deleted) {
                bail!("definition '{name}' is already defined");
            }
            Ok(Some(text.as_bytes().to_vec()))
        },
    )?;
    Ok(Some(root.join(locator)))
}

pub(super) fn edit(
    root: &Path,
    path: &Path,
    transform: impl FnOnce(&str) -> Result<String>,
) -> Result<WriteOutcome> {
    let rel = rel_for_path(root, path)?;
    let Some((mut store, repo)) = active(root, rel)? else {
        bail!("hierarchy authority changed while editing");
    };
    let locator = path
        .strip_prefix(root)?
        .to_str()
        .context("non-UTF-8 definition locator")?;
    let current = store
        .read(&repo, kind(rel)?, locator)?
        .filter(|doc| !doc.deleted)
        .context("definition no longer exists")?;
    current.ensure_understood()?;
    let original =
        std::str::from_utf8(&current.content).context("stored definition is not UTF-8")?;
    let next = transform(original)?;
    let changed = next != original;
    let (mapping, _) = split_frontmatter(&next);
    let name = yaml_string(&mapping, "name");
    let previous_name = yaml_string(&split_frontmatter(original).0, "name");
    store.mutate_checked(
        &repo,
        kind(rel)?,
        locator,
        Some(current.revision),
        |documents| match name {
            Some(name) if previous_name.as_deref() != Some(&name) => {
                validate_available_name(documents, &name, Some(locator))
            }
            Some(_) => Ok(()),
            None => Ok(()),
        },
        |_| Ok(Some(next.into_bytes())),
    )?;
    Ok(if changed {
        WriteOutcome::Changed
    } else {
        WriteOutcome::Unchanged
    })
}

fn validate_available_name(documents: &[Document], name: &str, except: Option<&str>) -> Result<()> {
    let slug = filename_slug(name);
    for doc in documents
        .iter()
        .filter(|doc| !doc.deleted && Some(doc.locator.as_str()) != except)
    {
        doc.ensure_understood()?;
        let content =
            std::str::from_utf8(&doc.content).context("stored definition is not UTF-8")?;
        let (mapping, _) = split_frontmatter(content);
        if yaml_string(&mapping, "name").as_deref() == Some(name) {
            bail!("definition '{name}' is already defined");
        }
        if Path::new(&doc.locator)
            .file_stem()
            .and_then(OsStr::to_str)
            .is_some_and(|stem| stem.eq_ignore_ascii_case(&slug))
        {
            bail!(
                "definition with the same filename slug already exists: {}",
                doc.locator
            );
        }
    }
    Ok(())
}

pub(super) fn validate_rename(root: &Path, old_path: &Path, new: &str) -> Result<()> {
    let Some((store, repo)) = active(root, PROJECTS_DIR)? else {
        bail!("hierarchy authority changed while planning rename");
    };
    let locator = old_path
        .strip_prefix(root)?
        .to_str()
        .context("non-UTF-8 definition locator")?;
    validate_available_name(&store.list(&repo, "project")?, new, Some(locator))
}

/// A rename cannot straddle filesystem and SQLite writers: no rollback can
/// span those stores. Require completion of the affected cutovers first.
pub(super) fn require_complete_rename_authority(root: &Path) -> Result<()> {
    let Some(store) = Store::open_existing_default()? else {
        return Ok(());
    };
    let Some(repo) = store.repository(root)? else {
        return Ok(());
    };
    let mut central = Vec::new();
    let mut legacy = Vec::new();
    for kind in ["project", "task", "goals", "ranking"] {
        if store.authority(&repo, kind)? {
            central.push(kind);
        } else {
            legacy.push(kind);
        }
    }
    if !central.is_empty() && !legacy.is_empty() {
        bail!("project rename requires one storage authority; migrate {} before renaming (already central: {})",
            legacy.join(", "), central.join(", "));
    }
    Ok(())
}
