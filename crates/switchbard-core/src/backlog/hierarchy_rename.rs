//! Atomic project rename when every affected kind is centrally authoritative.
use super::{
    filename_slug, split_frontmatter, transform_def, yaml_scalar, yaml_string, ProjectRename,
};
use crate::backlog::{central_commands, goals, parse, ranking, write};
use crate::storage::Document;
use anyhow::{bail, ensure, Context, Result};
use std::ffi::OsStr;
use std::path::Path;

pub(super) fn project(root: &Path, old: &str, new: &str) -> Result<Option<ProjectRename>> {
    let kinds = ["project", "task", "goals", "ranking"];
    let Some((mut store, repo)) = central_commands::command_store(root, &kinds)? else {
        return Ok(None);
    };
    let report = store.mutate_kinds(&repo, &kinds, None, |documents, _history| {
        validate_rename(documents, old, new)?;
        let mut report = ProjectRename::default();
        for document in documents.iter_mut().filter(|doc| !doc.deleted) {
            if document.kind == "project" && definition_name(document)? == old {
                let text = std::str::from_utf8(&document.content)?;
                document.content = transform_def(text, |fm, _| {
                    write::set_scalar(fm, "name", &yaml_scalar(new), None);
                    Ok(())
                })?
                .into_bytes();
                report.def_renamed = true;
            } else if document.kind == "task" && task_project(document)?.as_deref() == Some(old) {
                let text = std::str::from_utf8(&document.content)?;
                ensure!(
                    parse::body_round_trips(split_frontmatter(text).1),
                    "member task {} would not round-trip",
                    document.locator
                );
                let (text, outcome) = write::edit_text(text, |draft| {
                    write::set_task_project_draft(draft, Some(new))
                })?;
                document.content = text.into_bytes();
                report.tasks_updated += usize::from(outcome.changed());
            }
        }
        report.goals_updated =
            rename_aggregate(documents, "goals", old, new, goals::rename_project_document)?;
        report.ranking_updated = rename_aggregate(
            documents,
            "ranking",
            old,
            new,
            ranking::rename_project_document,
        )?;
        Ok(report)
    })?;
    Ok(Some(report))
}

fn validate_rename(documents: &[Document], old: &str, new: &str) -> Result<()> {
    let mut old_definitions = 0;
    let mut members = 0;
    let slug = filename_slug(new);
    for document in documents.iter().filter(|doc| !doc.deleted) {
        if document.kind == "project" {
            let name = definition_name(document)?;
            old_definitions += usize::from(name == old);
            ensure!(
                name != new,
                "project '{new}' is already defined; merging is not a rename"
            );
            if name != old
                && Path::new(&document.locator)
                    .file_stem()
                    .and_then(OsStr::to_str)
                    .is_some_and(|stem| stem.eq_ignore_ascii_case(&slug))
            {
                bail!(
                    "a project definition with the same filename slug already exists: {}",
                    document.locator
                );
            }
        } else if document.kind == "task" {
            let project = task_project(document)?;
            members += usize::from(project.as_deref() == Some(old));
            ensure!(
                project.as_deref() != Some(new),
                "project '{new}' is already referenced; merging is not a rename"
            );
        }
    }
    ensure!(
        old_definitions <= 1,
        "multiple project definitions claim '{old}'"
    );
    ensure!(
        old_definitions > 0 || members > 0,
        "no project named '{old}' - nothing defines or references it"
    );
    Ok(())
}

fn definition_name(document: &Document) -> Result<String> {
    let mapping = definition_mapping(document)?;
    yaml_string(&mapping, "name")
        .or_else(|| {
            Path::new(&document.locator)
                .file_stem()
                .and_then(OsStr::to_str)
                .map(str::to_owned)
        })
        .context("project definition has no name or logical file stem")
}

fn task_project(document: &Document) -> Result<Option<String>> {
    let mapping = definition_mapping(document)?;
    Ok(yaml_string(&mapping, "project").or_else(|| yaml_string(&mapping, "milestone")))
}

fn rename_aggregate(
    documents: &mut [Document],
    kind: &str,
    old: &str,
    new: &str,
    rename: impl FnOnce(Option<&[u8]>, &str, &str) -> Result<Option<Vec<u8>>>,
) -> Result<bool> {
    let original = documents
        .iter()
        .find(|doc| doc.kind == kind && !doc.deleted)
        .map(|doc| doc.content.clone());
    central_commands::rename_aggregate(documents, kind, old, new, rename)?;
    let next = documents
        .iter()
        .find(|doc| doc.kind == kind && !doc.deleted)
        .map(|doc| &doc.content);
    Ok(original.as_ref() != next)
}

fn definition_mapping(document: &Document) -> Result<serde_yaml::Mapping> {
    document.ensure_understood()?;
    let raw = write::split_raw(std::str::from_utf8(&document.content)?)?;
    serde_yaml::from_str(&raw.fm.join("\n"))
        .with_context(|| format!("cannot safely inspect references in {}", document.locator))
}
