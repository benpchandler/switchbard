//! Project declarations share task field kinds, but have an independent namespace.
//! Schema and value writes take the same repository lock, including after cutover.
use super::field_config::{
    fields_from_config_key, validate_decl_structure, validate_field_value, with_fields_key_edit,
    FieldDecl, FieldEditPatch,
};
use super::hierarchy::load_project_defs;
use anyhow::{anyhow, ensure, Result};
use std::collections::BTreeSet;
use std::path::Path;

const PROJECT_KEYS: &[&str] = &[
    "name",
    "status",
    "target_date",
    "initiative",
    "lead",
    "description",
    "path",
    "custom",
];

pub(super) fn validate_project_decl(decl: &FieldDecl) -> Result<()> {
    validate_decl_structure(decl)?;
    for value in &decl.values {
        let trimmed = super::write::validated_single_line("enum value", value)?;
        ensure!(
            trimmed == value,
            "enum values must not have surrounding whitespace"
        );
    }
    ensure!(
        !PROJECT_KEYS.contains(&decl.name.as_str()),
        "field `{}` collides with a built-in project key",
        decl.name
    );
    Ok(())
}

/// Load project declarations independently of task declarations.
pub fn declared_project_fields(root: &Path) -> Result<Vec<FieldDecl>> {
    let Some(text) = super::status_config::read_config(root)? else {
        return Ok(Vec::new());
    };
    Ok(fields_from_config_key(&text, "project_fields")
        .into_iter()
        .filter(|decl| validate_project_decl(decl).is_ok())
        .collect())
}

pub fn add_project_field_decl(root: &Path, decl: FieldDecl) -> Result<Vec<FieldDecl>> {
    let _lock = crate::storage::RepositoryLock::acquire(root)?;
    validate_project_decl(&decl)?;
    with_fields_key_edit(root, "project_fields", |fields| {
        ensure!(
            !fields.iter().any(|field| field.name == decl.name),
            "project field `{}` is already declared",
            decl.name
        );
        fields.push(decl);
        Ok(())
    })
}

pub fn edit_project_field_decl(
    root: &Path,
    name: &str,
    patch: &FieldEditPatch,
) -> Result<FieldDecl> {
    let _lock = crate::storage::RepositoryLock::acquire(root)?;
    let projects = checked_projects(root)?;
    let mut updated = None;
    with_fields_key_edit(root, "project_fields", |fields| {
        let field = fields
            .iter_mut()
            .find(|field| field.name == name)
            .ok_or_else(|| anyhow!("no project field named `{name}` is declared"))?;
        if let Some(values) = &patch.values {
            field.values.clone_from(values);
        }
        if let Some(groupable) = patch.groupable {
            field.groupable = groupable;
        }
        validate_project_decl(field)?;
        for project in &projects {
            if let Some(value) = project.custom.get(name) {
                validate_field_value(field, value).map_err(|error| {
                    anyhow!(
                        "project `{}` still uses field `{name}`: {error}",
                        project.name
                    )
                })?;
            }
        }
        updated = Some(field.clone());
        Ok(())
    })?;
    Ok(updated.expect("successful edit supplies updated declaration"))
}

pub fn remove_project_field_decl(root: &Path, name: &str) -> Result<()> {
    let _lock = crate::storage::RepositoryLock::acquire(root)?;
    let projects = checked_projects(root)?;
    let holders: Vec<_> = projects
        .iter()
        .filter(|project| project.custom.contains_key(name))
        .map(|project| project.name.as_str())
        .collect();
    ensure!(
        holders.is_empty(),
        "project field `{name}` is still set on: {}",
        holders.join(", ")
    );
    with_fields_key_edit(root, "project_fields", |fields| {
        let before = fields.len();
        fields.retain(|field| field.name != name);
        ensure!(
            fields.len() < before,
            "no project field named `{name}` is declared"
        );
        Ok(())
    })?;
    Ok(())
}

fn checked_projects(root: &Path) -> Result<Vec<super::hierarchy::ProjectDef>> {
    let mut warnings = Vec::new();
    let projects = load_project_defs(root, &mut warnings)?;
    ensure!(
        warnings.is_empty(),
        "cannot safely inspect project values: {}",
        warnings.join("; ")
    );
    Ok(projects)
}

pub(super) fn validate_project_field_patch(
    root: &Path,
    set: &[(String, String)],
    unset: &[String],
) -> Result<()> {
    let fields = declared_project_fields(root)?;
    let mut names = BTreeSet::new();
    for (name, value) in set {
        ensure!(
            names.insert(name),
            "project field `{name}` specified more than once"
        );
        let field = fields
            .iter()
            .find(|field| field.name == *name)
            .ok_or_else(|| anyhow!("no project field named `{name}` is declared"))?;
        super::write::validated_single_line(name, value)?;
        validate_field_value(field, value)?;
    }
    for name in unset {
        ensure!(
            names.insert(name),
            "project field `{name}` specified more than once or both set and unset"
        );
        ensure!(
            fields.iter().any(|field| field.name == *name),
            "no project field named `{name}` is declared"
        );
    }
    Ok(())
}

#[cfg(test)]
#[path = "project_field_tests.rs"]
mod tests;
