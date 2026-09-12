//! Repo-declared custom task fields (`backlog/config.yml`'s `fields:` list).
//!
//! A declaration is `{name, kind, values (enum only), groupable}`. Reading
//! follows `super::parse::parse_config_statuses`'s shape: a full read-only
//! `serde_yaml` parse, never fatal to the whole repo load — a missing,
//! unreadable, or malformed `fields:` key just yields an empty list.
//! Writing follows `super::status_config`'s surgical philosophy at *block*
//! granularity: the `fields:` key and everything indented under it is the
//! one span this module ever rewrites; every other key, comment, and byte
//! in `backlog/config.yml` survives untouched.
//!
//! # Why field names never carry a hyphen
//!
//! The declaration's `name` is also the frontmatter key `sb --set`/`--unset`
//! write through `super::write::set_scalar`/`remove_key`. Those functions
//! locate a key's span via `super::write::line_key`, which recognizes only
//! ASCII alphanumerics and `_` as valid top-level key characters — a hyphen
//! would never be found on a second write, so every edit after the first
//! would insert a duplicate `key: value` line rather than replacing it.
//! [`valid_field_name`] is therefore narrower than a generic YAML identifier:
//! `[a-z][a-z0-9_]*`, matching exactly what the write layer can address.

use super::parse::yaml_string;
use super::types::{BacklogRepo, BacklogTaskSource};
use anyhow::{anyhow, bail, ensure, Context, Result};
use serde_yaml::{Mapping, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

/// The frontmatter keys this crate already models. A declared field name
/// colliding with one of these would shadow a built-in scalar/list rather
/// than adding a new one — refused at declaration time. Mirrors exactly the
/// keys `super::parse::parse_task_text` reads (`yaml_string`/
/// `yaml_string_list` calls), which is the sole authority for this list.
pub const BUILTIN_FIELD_KEYS: &[&str] = &[
    "id",
    "title",
    "status",
    "priority",
    "assignee",
    "labels",
    "dependencies",
    "references",
    "project",
    "milestone",
    "parent",
    "parent_task_id",
    "created_date",
    "updated_date",
];

/// A custom field's value type. Determines what [`validate_field_value`]
/// accepts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldKind {
    /// Any non-empty string.
    Text,
    /// One of the declaration's own `values`, verbatim.
    Enum,
    /// `YYYY-MM-DD`.
    Date,
    /// A ball-style holder token: `[a-z0-9_-]+` (see `super::ball::is_holder`).
    Person,
}

impl FieldKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Enum => "enum",
            Self::Date => "date",
            Self::Person => "person",
        }
    }

    pub fn parse(word: &str) -> Result<Self> {
        match word.trim().to_ascii_lowercase().as_str() {
            "text" => Ok(Self::Text),
            "enum" => Ok(Self::Enum),
            "date" => Ok(Self::Date),
            "person" => Ok(Self::Person),
            other => bail!("unknown field kind `{other}` (expected text, enum, date, or person)"),
        }
    }
}

/// One repo-declared custom field. `values` is populated (and meaningful)
/// only for [`FieldKind::Enum`]; it also doubles as that field's sort/section
/// order, per the declaration contract.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldDecl {
    pub name: String,
    pub kind: FieldKind,
    pub values: Vec<String>,
    pub groupable: bool,
}

/// `[a-z][a-z0-9_]*` — see the module doc for why hyphens are excluded.
pub fn valid_field_name(name: &str) -> bool {
    let mut chars = name.chars();
    matches!(chars.next(), Some(first) if first.is_ascii_lowercase())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

/// Shape checks a declaration must pass before it can be written: a valid,
/// non-colliding name, and `values` present iff the kind is `enum`, with no
/// blank or duplicate entries.
fn validate_decl_shape(decl: &FieldDecl) -> Result<()> {
    ensure!(
        valid_field_name(&decl.name),
        "field name `{}` must start with a lowercase letter and contain only \
         lowercase letters, digits, or `_`",
        decl.name
    );
    ensure!(
        !BUILTIN_FIELD_KEYS.contains(&decl.name.as_str()),
        "field name `{}` collides with a built-in task key",
        decl.name
    );
    if decl.kind == FieldKind::Enum {
        ensure!(
            !decl.values.is_empty(),
            "enum field `{}` needs at least one value",
            decl.name
        );
        let mut seen = BTreeSet::new();
        for value in &decl.values {
            ensure!(
                !value.trim().is_empty(),
                "enum field `{}` cannot declare an empty value",
                decl.name
            );
            ensure!(
                seen.insert(value.as_str()),
                "enum field `{}` declares `{value}` more than once",
                decl.name
            );
        }
    } else {
        ensure!(
            decl.values.is_empty(),
            "only an enum field declares `values` (field `{}` is `{}`)",
            decl.name,
            decl.kind.as_str()
        );
    }
    Ok(())
}

/// Validate one candidate value against its declaration — the `sb` boundary
/// check (Rule 5: validate once, trust downstream). Never called by this
/// crate's own write path, which trusts whatever `sb` already validated.
pub fn validate_field_value(decl: &FieldDecl, value: &str) -> Result<()> {
    let trimmed = value.trim();
    match decl.kind {
        FieldKind::Text => {
            ensure!(
                !trimmed.is_empty(),
                "field `{}` cannot be set to an empty value",
                decl.name
            );
        }
        FieldKind::Enum => {
            ensure!(
                decl.values.iter().any(|v| v == trimmed),
                "field `{}` must be one of: {} (got `{trimmed}`)",
                decl.name,
                decl.values.join(", ")
            );
        }
        FieldKind::Date => {
            ensure!(
                chrono::NaiveDate::parse_from_str(trimmed, "%Y-%m-%d").is_ok(),
                "field `{}` must be a date in YYYY-MM-DD form (got `{trimmed}`)",
                decl.name
            );
        }
        FieldKind::Person => {
            ensure!(
                is_person_token(trimmed),
                "field `{}` must be a lowercase name using only letters, digits, `-`, \
                 or `_` (got `{trimmed}`)",
                decl.name
            );
        }
    }
    Ok(())
}

/// Where `value` sits in a declaration's own order. An enum field ranks by
/// position in its `values` list (the declaration contract makes that list the
/// field's sort *and* section order); an unknown value ranks after every
/// declared one. Every other kind has no declared order, so every value ranks
/// 0 and the caller's own tie-break (lexical) is what orders them.
///
/// The one definition of "a declared field's order": `sb list --sort` reaches
/// it through [`custom_field_sort_key`], and sbt's column registry ranks its
/// custom columns with it for sorting, grouping and value pickers.
pub fn declared_value_rank(decl: &FieldDecl, value: &str) -> usize {
    if decl.kind != FieldKind::Enum {
        return 0;
    }
    decl.values
        .iter()
        .position(|known| known == value)
        .unwrap_or(decl.values.len())
}

/// `(absent, declared rank, raw value)` — a task that does not set the field
/// sorts last whatever the direction, then [`declared_value_rank`] orders the
/// rest, then the raw value breaks the tie.
pub fn custom_field_sort_key<'t>(
    task: &'t crate::backlog::BacklogTask,
    decl: &FieldDecl,
) -> (bool, usize, &'t str) {
    match task.custom.get(&decl.name) {
        None => (true, usize::MAX, ""),
        Some(value) => (false, declared_value_rank(decl, value), value.as_str()),
    }
}

fn is_person_token(value: &str) -> bool {
    !value.is_empty()
        && value
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '-' | '_'))
}

/// Read `backlog/config.yml`'s `fields:` list — see module doc for why a
/// missing/malformed declaration is never fatal to the whole repo load.
pub(super) fn parse_field_decls(root: &Path) -> Result<Vec<FieldDecl>> {
    let Some(text) = super::status_config::read_config(root)? else {
        return Ok(Vec::new());
    };
    Ok(fields_from_config_text(&text))
}

/// This repo's declared custom fields alone, without loading any task data —
/// the cheap path for callers that only need the declarations (`sb`'s
/// `--set`/`--unset`/`--where` boundary validation, `sb field list`).
/// `BacklogRepo::fields` (from [`super::load_backlog_repo`]) is the same
/// data for a caller that is loading the full repo anyway.
pub fn declared_fields(root: &Path) -> Result<Vec<FieldDecl>> {
    parse_field_decls(root)
}

fn fields_from_config_text(text: &str) -> Vec<FieldDecl> {
    let Ok(value) = serde_yaml::from_str::<Value>(text) else {
        return Vec::new();
    };
    let Some(mapping) = value.as_mapping() else {
        return Vec::new();
    };
    let Some(Value::Sequence(items)) = mapping.get(Value::String("fields".to_string())) else {
        return Vec::new();
    };
    items.iter().filter_map(field_decl_from_yaml).collect()
}

fn field_decl_from_yaml(item: &Value) -> Option<FieldDecl> {
    let map = item.as_mapping()?;
    let name = map
        .get(Value::String("name".to_string()))?
        .as_str()?
        .trim()
        .to_string();
    if !valid_field_name(&name) {
        return None;
    }
    let kind = map
        .get(Value::String("kind".to_string()))
        .and_then(Value::as_str)
        .and_then(|k| FieldKind::parse(k).ok())?;
    let values = map
        .get(Value::String("values".to_string()))
        .and_then(Value::as_sequence)
        .map(|seq| {
            seq.iter()
                .filter_map(|v| v.as_str().map(|s| s.trim().to_string()))
                .filter(|s| !s.is_empty())
                .collect()
        })
        .unwrap_or_default();
    let groupable = map
        .get(Value::String("groupable".to_string()))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    Some(FieldDecl {
        name,
        kind,
        values,
        groupable,
    })
}

/// Every declared name a task's raw frontmatter carries a value for —
/// `super::parse::parse_task_text_with_fields`'s sole caller of this
/// pairing. Undeclared keys are never inspected here; they stay opaque and
/// round-trip through `super::write` untouched.
pub(super) fn extract_custom_fields(
    frontmatter: &Mapping,
    decls: &[FieldDecl],
) -> BTreeMap<String, String> {
    let mut custom = BTreeMap::new();
    for decl in decls {
        if let Some(value) = yaml_string(frontmatter, &decl.name) {
            custom.insert(decl.name.clone(), value);
        }
    }
    custom
}

/// Every task id whose source is `Active` or `Completed` and currently
/// sets `name` — the guard `sb field remove` (and any future caller) must
/// check before deleting a declaration, so a removal can never orphan data
/// a task still relies on. Deliberately wider than
/// `BacklogTaskSource::editable` (which is `Active`-only): a completed task
/// still carries its recorded custom-field value and removing the
/// declaration would silently drop it, so `Completed` must count as a
/// holder even though it can no longer be edited. Draft/archived tasks are
/// excluded — they're outside active management entirely.
pub fn tasks_setting_field<'r>(repo: &'r BacklogRepo, name: &str) -> Vec<&'r str> {
    repo.tasks
        .iter()
        .filter(|task| {
            matches!(
                task.source,
                BacklogTaskSource::Active | BacklogTaskSource::Completed
            )
        })
        .filter(|task| task.custom.contains_key(name))
        .map(|task| task.id.as_str())
        .collect()
}

/// Read-transform-write one atomic edit of the `fields:` block, through the
/// same central/legacy dual path `super::status_config::add_standard_statuses`
/// uses. Returns the resulting declaration list.
fn with_fields_edit(
    repo_root: &Path,
    transform: impl FnOnce(&mut Vec<FieldDecl>) -> Result<()>,
) -> Result<Vec<FieldDecl>> {
    super::aggregate_storage::with_edit(repo_root, "config", "backlog/config.yml", |edit| {
        let path = if edit.is_central() {
            repo_root.join("backlog/config.yml")
        } else {
            super::status_config::config_path(repo_root)
                .ok_or_else(|| anyhow!("no backlog config found under {}", repo_root.display()))?
        };
        let original = if edit.is_central() && !edit.exists(&path) {
            String::new()
        } else {
            edit.text(&path)?
        };
        let mut fields = fields_from_config_text(&original);
        transform(&mut fields)?;
        let out = splice_fields_block(&original, &fields);
        if !edit.stage(&out) {
            super::write::atomic_write(&path, &out)
                .with_context(|| format!("writing {}", path.display()))?;
        }
        Ok(fields)
    })
}

/// Declare a new field. Refuses a name that fails [`validate_decl_shape`] or
/// is already declared.
pub fn add_field_decl(repo_root: &Path, decl: FieldDecl) -> Result<Vec<FieldDecl>> {
    validate_decl_shape(&decl)?;
    with_fields_edit(repo_root, |fields| {
        ensure!(
            !fields.iter().any(|f| f.name == decl.name),
            "field `{}` is already declared",
            decl.name
        );
        fields.push(decl.clone());
        Ok(())
    })
}

/// Values and/or groupable to overwrite on an existing declaration; `None`
/// leaves that part of the declaration untouched.
#[derive(Debug, Clone, Default)]
pub struct FieldEditPatch {
    pub values: Option<Vec<String>>,
    pub groupable: Option<bool>,
}

/// Edit an existing declaration in place (kind and name are immutable —
/// remove and re-add to change either). Refuses if the result would fail
/// [`validate_decl_shape`] (e.g. clearing `values` on an enum field).
pub fn edit_field_decl(repo_root: &Path, name: &str, patch: &FieldEditPatch) -> Result<FieldDecl> {
    let mut updated = None;
    with_fields_edit(repo_root, |fields| {
        let field = fields
            .iter_mut()
            .find(|f| f.name == name)
            .ok_or_else(|| anyhow!("no field named `{name}` is declared"))?;
        if let Some(values) = &patch.values {
            field.values = values.clone();
        }
        if let Some(groupable) = patch.groupable {
            field.groupable = groupable;
        }
        validate_decl_shape(field)?;
        updated = Some(field.clone());
        Ok(())
    })?;
    Ok(updated.expect("set by the closure on every non-error path"))
}

/// Remove a declaration. Callers own the "no task still sets it" guard —
/// see [`tasks_setting_field`] — this function only refuses an unknown name.
pub fn remove_field_decl(repo_root: &Path, name: &str) -> Result<()> {
    with_fields_edit(repo_root, |fields| {
        let before = fields.len();
        fields.retain(|f| f.name != name);
        ensure!(fields.len() < before, "no field named `{name}` is declared");
        Ok(())
    })?;
    Ok(())
}

fn render_fields_block(fields: &[FieldDecl]) -> Vec<String> {
    if fields.is_empty() {
        return vec!["fields: []".to_string()];
    }
    let mut lines = vec!["fields:".to_string()];
    for field in fields {
        lines.push(format!("  - name: {}", field.name));
        lines.push(format!("    kind: {}", field.kind.as_str()));
        if !field.values.is_empty() {
            let rendered = field
                .values
                .iter()
                .map(|v| quote_yaml(v))
                .collect::<Vec<_>>()
                .join(", ");
            lines.push(format!("    values: [{rendered}]"));
        }
        lines.push(format!("    groupable: {}", field.groupable));
    }
    lines
}

fn quote_yaml(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

/// `[start, end)` of the lines belonging to the `fields:` key: either a
/// single line (`fields: []`, or any other single-line form a hand-authored
/// config might use) or the `fields:` line plus every indented line after
/// it. `None` when the key is absent.
///
/// Deliberately not a YAML parse-then-reserialize of the whole document —
/// see `super::status_config::parse_statuses_line`'s doc for why: it would
/// reformat every other key and strip comments.
fn fields_block_span(lines: &[&str]) -> Option<(usize, usize)> {
    let start = lines
        .iter()
        .position(|l| *l == "fields:" || l.starts_with("fields: "))?;
    if lines[start] != "fields:" {
        return Some((start, start + 1));
    }
    let mut end = start + 1;
    while end < lines.len() && (lines[end].starts_with(' ') || lines[end].starts_with('\t')) {
        end += 1;
    }
    Some((start, end))
}

fn splice_fields_block(original: &str, fields: &[FieldDecl]) -> String {
    let lines: Vec<&str> = original.lines().collect();
    let rendered = render_fields_block(fields);
    let mut out_lines: Vec<String> = Vec::with_capacity(lines.len() + rendered.len());
    match fields_block_span(&lines) {
        Some((start, end)) => {
            out_lines.extend(lines[..start].iter().map(|s| (*s).to_string()));
            out_lines.extend(rendered);
            out_lines.extend(lines[end..].iter().map(|s| (*s).to_string()));
        }
        None => {
            out_lines.extend(lines.iter().map(|s| (*s).to_string()));
            out_lines.extend(rendered);
        }
    }
    let mut out = out_lines.join("\n");
    if original.ends_with('\n') || original.is_empty() {
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backlog::BacklogTaskSource;
    use std::path::PathBuf;
    use tempfile::TempDir;

    fn repo_with_config(body: &str) -> (TempDir, PathBuf) {
        let tmp = TempDir::new().unwrap();
        std::fs::create_dir(tmp.path().join("backlog")).unwrap();
        let path = tmp.path().join("backlog/config.yml");
        std::fs::write(&path, body).unwrap();
        let root = tmp.path().to_path_buf();
        (tmp, root)
    }

    fn enum_decl(name: &str, values: &[&str]) -> FieldDecl {
        FieldDecl {
            name: name.to_string(),
            kind: FieldKind::Enum,
            values: values.iter().map(|v| (*v).to_string()).collect(),
            groupable: true,
        }
    }

    const TRIO: &str = "project_name: \"Demo\"\n\
                        statuses: [\"To Do\", \"Done\"]\n\
                        default_editor: \"hx\"\n";

    #[test]
    fn valid_field_name_accepts_lowercase_alnum_underscore_only() {
        assert!(valid_field_name("counterparty"));
        assert!(valid_field_name("loan_type_2"));
        assert!(!valid_field_name("Counterparty"), "no uppercase");
        assert!(!valid_field_name("2loans"), "must start with a letter");
        assert!(!valid_field_name("loan-type"), "no hyphen — see module doc");
        assert!(!valid_field_name(""), "not empty");
    }

    #[test]
    fn add_field_decl_writes_the_block_and_preserves_every_other_line() {
        let (_tmp, root) = repo_with_config(TRIO);
        let decl = enum_decl("counterparty", &["GoSBA Loans", "Nick"]);
        let result = add_field_decl(&root, decl).unwrap();
        assert_eq!(result.len(), 1);

        let after = std::fs::read_to_string(root.join("backlog/config.yml")).unwrap();
        for line in TRIO.lines() {
            assert!(after.contains(line), "missing original line: {line}");
        }
        assert!(after.contains("fields:\n  - name: counterparty\n    kind: enum\n"));
        assert!(after.contains("values: [\"GoSBA Loans\", \"Nick\"]"));
        assert!(after.contains("groupable: true"));
    }

    #[test]
    fn add_field_decl_rejects_a_malformed_or_colliding_name() {
        let (_tmp, root) = repo_with_config(TRIO);
        assert!(add_field_decl(&root, enum_decl("Bad-Name", &["x"])).is_err());
        assert!(add_field_decl(&root, enum_decl("status", &["x"])).is_err());
        let empty_enum = FieldDecl {
            name: "empty_enum".to_string(),
            kind: FieldKind::Enum,
            values: vec![],
            groupable: false,
        };
        assert!(
            add_field_decl(&root, empty_enum).is_err(),
            "an enum field needs at least one value"
        );
    }

    #[test]
    fn add_field_decl_rejects_a_duplicate_declaration() {
        let (_tmp, root) = repo_with_config(TRIO);
        add_field_decl(&root, enum_decl("counterparty", &["A"])).unwrap();
        assert!(add_field_decl(&root, enum_decl("counterparty", &["B"])).is_err());
    }

    #[test]
    fn edit_field_decl_replaces_values_and_validates_the_result() {
        let (_tmp, root) = repo_with_config(TRIO);
        add_field_decl(&root, enum_decl("counterparty", &["A", "B"])).unwrap();
        let updated = edit_field_decl(
            &root,
            "counterparty",
            &FieldEditPatch {
                values: Some(vec!["A".to_string(), "B".to_string(), "C".to_string()]),
                groupable: Some(false),
            },
        )
        .unwrap();
        assert_eq!(updated.values, vec!["A", "B", "C"]);
        assert!(!updated.groupable);

        let cleared = edit_field_decl(
            &root,
            "counterparty",
            &FieldEditPatch {
                values: Some(vec![]),
                groupable: None,
            },
        );
        assert!(cleared.is_err(), "an enum field cannot lose every value");
    }

    #[test]
    fn edit_field_decl_refuses_an_unknown_name() {
        let (_tmp, root) = repo_with_config(TRIO);
        assert!(edit_field_decl(&root, "ghost", &FieldEditPatch::default()).is_err());
    }

    #[test]
    fn remove_field_decl_drops_only_the_named_entry() {
        let (_tmp, root) = repo_with_config(TRIO);
        add_field_decl(&root, enum_decl("counterparty", &["A"])).unwrap();
        add_field_decl(&root, enum_decl("stage", &["Draft", "Signed"])).unwrap();
        remove_field_decl(&root, "counterparty").unwrap();
        let remaining = parse_field_decls(&root).unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].name, "stage");
    }

    #[test]
    fn remove_field_decl_refuses_an_unknown_name() {
        let (_tmp, root) = repo_with_config(TRIO);
        assert!(remove_field_decl(&root, "ghost").is_err());
    }

    #[test]
    fn a_repo_declaring_no_fields_round_trips_with_zero_lines_changed() {
        let (_tmp, root) = repo_with_config(TRIO);
        let before = std::fs::read_to_string(root.join("backlog/config.yml")).unwrap();
        assert_eq!(parse_field_decls(&root).unwrap(), Vec::new());
        let after = std::fs::read_to_string(root.join("backlog/config.yml")).unwrap();
        assert_eq!(before, after, "reading never writes");
    }

    #[test]
    fn validate_field_value_covers_every_kind() {
        let enum_field = enum_decl("counterparty", &["GoSBA Loans", "Nick"]);
        assert!(validate_field_value(&enum_field, "Nick").is_ok());
        let err = validate_field_value(&enum_field, "Bogus")
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("GoSBA Loans"),
            "names the allowed values: {err}"
        );
        assert!(err.contains("Nick"));

        let text_field = FieldDecl {
            name: "note".to_string(),
            kind: FieldKind::Text,
            values: vec![],
            groupable: false,
        };
        assert!(validate_field_value(&text_field, "anything").is_ok());
        assert!(validate_field_value(&text_field, "   ").is_err());

        let date_field = FieldDecl {
            name: "closes".to_string(),
            kind: FieldKind::Date,
            values: vec![],
            groupable: false,
        };
        assert!(validate_field_value(&date_field, "2026-09-11").is_ok());
        assert!(validate_field_value(&date_field, "09/11/2026").is_err());
        assert!(validate_field_value(&date_field, "2026-13-40").is_err());

        let person_field = FieldDecl {
            name: "reviewer".to_string(),
            kind: FieldKind::Person,
            values: vec![],
            groupable: false,
        };
        assert!(validate_field_value(&person_field, "nick-doe").is_ok());
        assert!(validate_field_value(&person_field, "Nick Doe").is_err());
    }

    fn task_with_custom(
        id: &str,
        source: BacklogTaskSource,
        custom: &[(&str, &str)],
    ) -> super::super::types::BacklogTask {
        super::super::types::BacklogTask {
            storage_identity: None,
            id: id.to_string(),
            title: "Fixture".to_string(),
            status: "To Do".to_string(),
            priority: "medium".to_string(),
            assignees: vec![],
            labels: vec![],
            dependencies: vec![],
            references: vec![],
            project: None,
            parent: None,
            created_date: None,
            updated_date: None,
            due_date: None,
            description: String::new(),
            implementation_plan: String::new(),
            implementation_notes: String::new(),
            final_summary: String::new(),
            acceptance_criteria: vec![],
            definition_of_done: vec![],
            source,
            path: PathBuf::new(),
            custom: custom
                .iter()
                .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
                .collect(),
        }
    }

    #[test]
    fn tasks_setting_field_only_counts_active_and_completed() {
        let repo = BacklogRepo {
            root: PathBuf::from("/repo"),
            tasks: vec![
                task_with_custom(
                    "TASK-1",
                    BacklogTaskSource::Active,
                    &[("counterparty", "Nick")],
                ),
                task_with_custom(
                    "TASK-2",
                    BacklogTaskSource::Completed,
                    &[("counterparty", "GoSBA Loans")],
                ),
                task_with_custom(
                    "TASK-3",
                    BacklogTaskSource::Archived,
                    &[("counterparty", "Nick")],
                ),
                task_with_custom("TASK-4", BacklogTaskSource::Active, &[]),
            ],
            warnings: vec![],
            project_defs: vec![],
            initiative_defs: vec![],
            goals: vec![],
            ranking: crate::backlog::RepoRanking::default(),
            loaded_at_unix: 0,
            configured_statuses: vec![],
            fields: vec![],
        };
        let mut ids = tasks_setting_field(&repo, "counterparty");
        ids.sort_unstable();
        assert_eq!(ids, vec!["TASK-1", "TASK-2"]);
        assert!(tasks_setting_field(&repo, "no-such-field").is_empty());
    }
}
