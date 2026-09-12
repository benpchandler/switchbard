//! Validate native document semantics at the checked storage transaction boundary.
//! Unknown content remains raw and opaque; this module never rewrites a payload.
use super::parse::{parse_task_text, yaml_string, yaml_string_list};
use super::{BacklogTask, BacklogTaskSource};
use crate::storage::{ExchangeRecord, ExchangeSnapshot};
use anyhow::{bail, ensure, Context, Result};
use serde_yaml::{Mapping, Value};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::Path;

/// Validate the complete candidate, before checked migration/import commits authority.
/// Missing legacy relationships stay unresolved; only local resolved edges form graphs.
pub fn validate_storage_snapshot(snapshot: &ExchangeSnapshot) -> Result<()> {
    snapshot.validate()?;
    let mut tasks = Vec::new();
    let mut names = BTreeSet::new();
    let mut prefix = "TASK".to_string();
    for record in &snapshot.records {
        if !crate::storage::content_is_understood(&record.kind, record.content_version) {
            continue;
        }
        let Some(source) = validate_locator(record)? else {
            continue;
        };
        let bytes = record.content.bytes()?;
        let text = std::str::from_utf8(&bytes)
            .with_context(|| format!("native document is not UTF-8: {}", record.locator))?;
        let mapping = if source == NativeSource::Aggregate {
            yaml_mapping(text)
                .with_context(|| format!("invalid native YAML: {}", record.locator))?
        } else {
            markdown_mapping(text)
                .with_context(|| format!("invalid frontmatter: {}", record.locator))?
        };
        match record.kind.as_str() {
            "task" => {
                validate_task_fields(&mapping)?;
                let (mut task, _) =
                    parse_task_text(Path::new(&record.locator), source.task_source(), text)?;
                // Strict frontmatter parsing also recognizes CRLF without changing source bytes.
                if let Some(id) = yaml_string(&mapping, "id") {
                    task.id = id;
                }
                task.parent = yaml_string(&mapping, "parent_task_id")
                    .or_else(|| yaml_string(&mapping, "parent"));
                task.dependencies = yaml_string_list(&mapping, "dependencies");
                tasks.push((task, record.deleted));
            }
            "initiative" | "project" => {
                for key in ["name", "status", "target_date", "initiative", "lead"] {
                    optional_scalar(&mapping, key)?;
                }
                let name = yaml_string(&mapping, "name").unwrap_or_else(|| {
                    Path::new(&record.locator)
                        .file_stem()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .into_owned()
                });
                ensure!(
                    names.insert((record.kind.clone(), name.clone())),
                    "duplicate {} name: {name}",
                    record.kind
                );
            }
            "config" => {
                ensure!(
                    !record.deleted,
                    "task config cannot be deleted; reset its payload"
                );
                if let Some(prefix) =
                    value(&mapping, "task_prefix").filter(|value| !value.is_null())
                {
                    ensure!(
                        prefix.is_string(),
                        "configured task prefix must be a string"
                    );
                }
                optional_list(&mapping, "statuses", true)?;
                if let Some(value) = yaml_string(&mapping, "task_prefix") {
                    ensure!(
                        !value.contains(['/', '\\', '\0', '\n', '\r']),
                        "unsafe configured task prefix"
                    );
                    prefix = value.to_uppercase();
                }
            }
            "goals" => validate_goals(&mapping)?,
            "ranking" => validate_ranking(&mapping)?,
            _ => unreachable!("native locator classification"),
        }
    }
    validate_task_graphs(&tasks, &prefix)
}

#[derive(PartialEq, Eq)]
enum NativeSource {
    Active,
    Completed,
    Draft,
    Archived,
    Definition,
    Aggregate,
}
impl NativeSource {
    fn task_source(&self) -> BacklogTaskSource {
        match self {
            Self::Completed => BacklogTaskSource::Completed,
            Self::Draft => BacklogTaskSource::Draft,
            Self::Archived => BacklogTaskSource::Archived,
            _ => BacklogTaskSource::Active,
        }
    }
}

fn validate_locator(record: &ExchangeRecord) -> Result<Option<NativeSource>> {
    let kind = record.kind.as_str();
    if !matches!(
        kind,
        "task" | "initiative" | "project" | "config" | "goals" | "ranking"
    ) {
        return Ok(None);
    }
    let path = record.locator.as_str();
    ensure!(
        !path.contains(['\\', ':', '\0'])
            && path.split('/').all(|part| !matches!(part, "" | "." | "..")),
        "unsafe native locator: {path}"
    );
    let source = match kind {
        "config" | "goals" | "ranking" => {
            let expected = match kind {
                "config" => "backlog/config.yml",
                "goals" => "backlog/goals.yml",
                _ => "backlog/ranking.yml",
            };
            ensure!(path == expected, "invalid {kind} singleton locator: {path}");
            NativeSource::Aggregate
        }
        _ => {
            let (parent, file) = path
                .rsplit_once('/')
                .context("native Markdown locator needs its directory")?;
            ensure!(
                file.ends_with(".md") && file.len() > 3,
                "native document requires .md filename: {path}"
            );
            match (kind, parent) {
                ("task", "backlog/tasks") => NativeSource::Active,
                ("task", "backlog/completed") => NativeSource::Completed,
                ("task", "backlog/drafts") => NativeSource::Draft,
                ("task", "backlog/archive/tasks") => NativeSource::Archived,
                ("initiative", "backlog/initiatives") | ("project", "backlog/projects") => {
                    NativeSource::Definition
                }
                _ => bail!("native locator is outside its kind directory: {path}"),
            }
        }
    };
    Ok(Some(source))
}

fn yaml_mapping(text: &str) -> Result<Mapping> {
    let value: Value = serde_yaml::from_str(text)?;
    value
        .as_mapping()
        .cloned()
        .context("native YAML must be a mapping")
}

fn markdown_mapping(text: &str) -> Result<Mapping> {
    let mut lines = text.split_inclusive('\n');
    let first = lines.next().unwrap_or("");
    if first.trim_end_matches(['\r', '\n']) != "---" {
        return Ok(Mapping::new());
    }
    let start = first.len();
    let mut end = start;
    for line in lines {
        if line.trim_end_matches(['\r', '\n']) == "---" {
            let yaml = &text[start..end];
            return if yaml.trim().is_empty() {
                Ok(Mapping::new())
            } else {
                yaml_mapping(yaml)
            };
        }
        end += line.len();
    }
    bail!("unterminated YAML frontmatter")
}

fn value<'a>(mapping: &'a Mapping, key: &str) -> Option<&'a Value> {
    mapping.get(Value::String(key.into()))
}
fn scalar(value: &Value) -> bool {
    matches!(value, Value::String(_) | Value::Number(_) | Value::Bool(_))
}
fn optional_scalar(mapping: &Mapping, key: &str) -> Result<()> {
    if let Some(value) = value(mapping, key).filter(|v| !v.is_null()) {
        ensure!(scalar(value), "native field {key} must be scalar");
    }
    Ok(())
}
fn optional_list(mapping: &Mapping, key: &str, comma_string: bool) -> Result<()> {
    if let Some(value) = value(mapping, key) {
        if comma_string && value.is_null() {
            return Ok(());
        }
        let valid = if comma_string {
            value
                .as_sequence()
                .is_some_and(|items| items.iter().all(scalar))
                || value.is_string()
        } else {
            // Match the aggregate loader's Vec<String> without rewriting raw values.
            serde_yaml::from_value::<Vec<String>>(value.clone()).is_ok()
        };
        ensure!(valid, "native field {key} must be a scalar list");
    }
    Ok(())
}

fn validate_task_fields(mapping: &Mapping) -> Result<()> {
    for key in [
        "id",
        "title",
        "status",
        "priority",
        "project",
        "milestone",
        "parent_task_id",
        "parent",
        "created_date",
        "updated_date",
    ] {
        optional_scalar(mapping, key)?;
    }
    for key in ["assignee", "labels", "dependencies", "references"] {
        optional_list(mapping, key, true)?;
    }
    Ok(())
}

fn validate_ranking(mapping: &Mapping) -> Result<()> {
    for key in ["expedite", "projects", "root_tasks"] {
        optional_list(mapping, key, false)?;
    }
    for key in ["tasks", "subissues"] {
        if let Some(value) = value(mapping, key) {
            let lanes = value
                .as_mapping()
                .context("ranking lanes must be a mapping")?;
            for (name, entries) in lanes {
                ensure!(
                    name.is_string()
                        && entries
                            .as_sequence()
                            .is_some_and(|items| items.iter().all(Value::is_string)),
                    "ranking lanes require string keys and string lists"
                );
            }
        }
    }
    Ok(())
}

fn validate_goals(mapping: &Mapping) -> Result<()> {
    let Some(goals) = value(mapping, "goals") else {
        return Ok(());
    };
    let goals = goals.as_sequence().context("goals must be a sequence")?;
    let mut names = BTreeSet::new();
    for goal in goals {
        let goal = goal.as_mapping().context("goal must be a mapping")?;
        let name = value(goal, "name")
            .and_then(Value::as_str)
            .filter(|s| !s.trim().is_empty())
            .context("goal requires a name")?;
        ensure!(names.insert(name), "duplicate goal name: {name}");
        if let Some(unit) = value(goal, "unit") {
            ensure!(unit.is_string(), "goal unit must be a string");
        }
        for key in ["scope", "measure"] {
            if let Some(value) = value(goal, key).filter(|v| !v.is_null()) {
                ensure!(value.is_string(), "goal {key} must be a string");
            }
        }
        if let Some(measure) = value(goal, "measure").and_then(Value::as_str) {
            ensure!(
                matches!(measure, "manual" | "tasks"),
                "unsupported goal measure: {measure}"
            );
        }
        if let Some(inputs) = value(goal, "inputs").filter(|v| !v.is_null()) {
            let inputs = inputs
                .as_mapping()
                .context("goal inputs must be a mapping")?;
            for key in ["tasks", "projects"] {
                optional_list(inputs, key, false)?;
            }
        }
        if let Some(weeks) = value(goal, "weeks") {
            for (week, content) in weeks.as_mapping().context("goal weeks must be a mapping")? {
                ensure!(week.is_string(), "goal week must be a string");
                validate_goal_week(
                    content
                        .as_mapping()
                        .context("goal week content must be a mapping")?,
                )?;
            }
        }
    }
    Ok(())
}
fn validate_goal_week(week: &Mapping) -> Result<()> {
    ensure!(
        value(week, "target").and_then(Value::as_i64).is_some(),
        "goal target must be an integer"
    );
    if let Some(checkins) = value(week, "checkins") {
        for checkin in checkins
            .as_sequence()
            .context("checkins must be a sequence")?
        {
            let checkin = checkin.as_mapping().context("checkin must be a mapping")?;
            ensure!(
                value(checkin, "date").is_some_and(Value::is_string),
                "checkin date must be a string"
            );
            ensure!(
                value(checkin, "value").and_then(Value::as_i64).is_some(),
                "checkin value must be an integer"
            );
        }
    }
    Ok(())
}

fn task_key(id: &str, prefix: &str) -> String {
    let upper = id.trim().to_uppercase();
    upper
        .strip_prefix(&format!("{prefix}-"))
        .or_else(|| upper.strip_prefix("TASK-"))
        .unwrap_or(&upper)
        .to_owned()
}
fn validate_task_graphs(tasks: &[(BacklogTask, bool)], prefix: &str) -> Result<()> {
    let mut ids = BTreeSet::new();
    let mut parents = BTreeMap::new();
    let mut dependencies = BTreeMap::new();
    for (task, deleted) in tasks {
        let id = task_key(&task.id, prefix);
        ensure!(
            !id.is_empty() && ids.insert(id.clone()),
            "duplicate task public ID: {}",
            task.id
        );
        if *deleted {
            continue;
        }
        let parent = task
            .parent
            .clone()
            .or_else(|| task.id.rsplit_once('.').map(|(parent, _)| parent.into()));
        parents.insert(
            id.clone(),
            parent.into_iter().map(|p| task_key(&p, prefix)).collect(),
        );
        dependencies.insert(
            id,
            task.dependencies
                .iter()
                .map(|d| task_key(d, prefix))
                .collect(),
        );
    }
    acyclic(&parents, "parent")?;
    acyclic(&dependencies, "dependency")
}
fn acyclic(edges: &BTreeMap<String, Vec<String>>, relation: &str) -> Result<()> {
    let mut indegree: BTreeMap<&String, usize> = edges.keys().map(|key| (key, 0)).collect();
    for targets in edges.values() {
        for target in targets {
            if let Some(degree) = indegree.get_mut(target) {
                *degree += 1;
            }
        }
    }
    let mut queue: VecDeque<&String> = indegree
        .iter()
        .filter_map(|(key, degree)| (*degree == 0).then_some(*key))
        .collect();
    let mut visited = 0;
    while let Some(node) = queue.pop_front() {
        visited += 1;
        for target in &edges[node] {
            if let Some(degree) = indegree.get_mut(target) {
                *degree -= 1;
                if *degree == 0 {
                    queue.push_back(target);
                }
            }
        }
    }
    ensure!(
        visited == edges.len(),
        "resolved {relation} cycle in native task graph"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::{ExchangeContent, RepositoryId, Store};
    use uuid::Uuid;

    fn record(kind: &str, locator: &str, bytes: &[u8]) -> ExchangeRecord {
        ExchangeRecord {
            content_version: 1,
            id: Uuid::new_v4().to_string(),
            kind: kind.into(),
            locator: locator.into(),
            content: ExchangeContent::from_bytes(bytes),
            deleted: false,
            clock: BTreeMap::from([(Uuid::new_v4().to_string(), 1)]),
        }
    }
    fn snapshot(records: Vec<ExchangeRecord>) -> ExchangeSnapshot {
        let mut snapshot = ExchangeSnapshot {
            version: 2,
            repo_id: RepositoryId(Uuid::new_v4().to_string()),
            epoch_id: Uuid::new_v4().to_string(),
            kinds: records
                .iter()
                .map(|r| r.kind.clone())
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect(),
            records,
            digest: String::new(),
        };
        snapshot.refresh_digest().unwrap();
        snapshot
    }
    fn task(id: &str, parent: &str, lifecycle: &str) -> ExchangeRecord {
        record(
            "task",
            &format!("backlog/{lifecycle}/{id}.md"),
            format!("---\nid: {id}\ntitle: Task\nparent_task_id: '{parent}'\n---\n").as_bytes(),
        )
    }

    #[test]
    fn native_paths_are_confined_and_unknown_kind_stays_opaque() {
        for path in [
            "../outside.md",
            "/backlog/tasks/one.md",
            "backlog/tasks/../../outside.md",
            "backlog/tasks/nested/one.md",
            "backlog/projects/one.md",
            "backlog/tasks/one.txt",
            "backlog\\tasks\\one.md",
            "C:/backlog/tasks/one.md",
        ] {
            assert!(
                validate_storage_snapshot(&snapshot(vec![record("task", path, b"plain text")]))
                    .is_err(),
                "{path}"
            );
        }
        let opaque = snapshot(vec![record("custom-kind", "opaque-locator", &[255, 0, 1])]);
        assert!(validate_storage_snapshot(&opaque).is_ok());
        assert_eq!(opaque.records[0].content.bytes().unwrap(), vec![255, 0, 1]);
    }

    #[test]
    fn duplicate_task_ids_across_lifecycles_and_definition_names_reject() {
        let tasks = snapshot(vec![
            task("TASK-1", "", "tasks"),
            task("task-1", "", "completed"),
        ]);
        assert!(validate_storage_snapshot(&tasks)
            .unwrap_err()
            .to_string()
            .contains("duplicate task public ID"));
        let definitions = snapshot(vec![
            record(
                "project",
                "backlog/projects/a.md",
                b"---\nname: Same\n---\n",
            ),
            record(
                "project",
                "backlog/projects/b.md",
                b"---\nname: Same\n---\n",
            ),
        ]);
        assert!(validate_storage_snapshot(&definitions).is_err());
        let separate_kinds = snapshot(vec![
            record(
                "project",
                "backlog/projects/a.md",
                b"---\nname: Same\n---\n",
            ),
            record(
                "initiative",
                "backlog/initiatives/a.md",
                b"---\nname: Same\n---\n",
            ),
        ]);
        assert!(validate_storage_snapshot(&separate_kinds).is_ok());
    }

    #[test]
    fn custom_nested_content_and_custom_status_survive_validation_exactly() {
        let raw = b"---\nid: TASK-1\nstatus: Waiting for a bespoke review\ncustom: {list: [null, '', [], {}, 3.14], nested: {key: value}}\n---\n## Unknown section\nKeep this byte-for-byte.\n";
        let input = snapshot(vec![record("task", "backlog/tasks/one.md", raw)]);
        let original = input.clone();
        validate_storage_snapshot(&input).unwrap();
        assert_eq!(input, original);
        assert_eq!(input.records[0].content.bytes().unwrap(), raw);
    }

    #[test]
    fn malformed_known_yaml_rejects_instead_of_falling_back_to_empty() {
        for raw in [
            b"---\nid: [unterminated\n---\n".as_slice(),
            b"---\n- not a mapping\n---\n",
            b"---\nid: TASK-1\n",
            b"---\nid: {invalid: id}\n---\n",
        ] {
            assert!(validate_storage_snapshot(&snapshot(vec![record(
                "task",
                "backlog/tasks/one.md",
                raw
            )]))
            .is_err());
        }
        assert!(validate_storage_snapshot(&snapshot(vec![record(
            "config",
            "backlog/config.yml",
            b"- not a mapping\n"
        )]))
        .is_err());
    }

    #[test]
    fn config_and_aggregate_singleton_semantics_are_checked() {
        let valid = snapshot(vec![
            record(
                "config",
                "backlog/config.yml",
                b"task_prefix: CUSTOM\nstatuses: [Backlog, Custom]\ncustom: {untouched: [null]}\n",
            ),
            record(
                "goals",
                "backlog/goals.yml",
                b"goals: [{name: Ship, weeks: {'2026-09-07': {target: 3}}}]\ncustom: untouched\n",
            ),
            record(
                "ranking",
                "backlog/ranking.yml",
                b"projects: [P]\ntasks: {P: [TASK-1]}\ncustom: {retained: true}\n",
            ),
        ]);
        validate_storage_snapshot(&valid).unwrap();
        for (kind, path, raw) in [
            ("config", "backlog/other.yml", "{}"),
            (
                "goals",
                "backlog/goals.yml",
                "goals: [{name: A, weeks: {week: {target: nope}}}]",
            ),
            ("ranking", "backlog/ranking.yml", "tasks: [not, lanes]"),
            (
                "goals",
                "backlog/goals.yml",
                "goals: [{name: A, unit: null}]",
            ),
        ] {
            assert!(
                validate_storage_snapshot(&snapshot(vec![record(kind, path, raw.as_bytes())]))
                    .is_err(),
                "{kind}: {raw}"
            );
        }
        let mut deleted = record("config", "backlog/config.yml", b"{}");
        deleted.deleted = true;
        assert!(validate_storage_snapshot(&snapshot(vec![deleted])).is_err());
    }

    #[test]
    fn resolved_cycles_reject_but_optional_missing_targets_remain_raw() {
        let cycle = snapshot(vec![
            task("TASK-1", "TASK-2", "tasks"),
            task("TASK-2", "TASK-1", "tasks"),
        ]);
        assert!(validate_storage_snapshot(&cycle)
            .unwrap_err()
            .to_string()
            .contains("parent cycle"));
        let dependency_cycle = snapshot(vec![
            record(
                "task",
                "backlog/tasks/a.md",
                b"---\nid: TASK-1\ndependencies: [TASK-2]\n---\n",
            ),
            record(
                "task",
                "backlog/tasks/b.md",
                b"---\nid: TASK-2\ndependencies: [TASK-1]\n---\n",
            ),
        ]);
        assert!(validate_storage_snapshot(&dependency_cycle)
            .unwrap_err()
            .to_string()
            .contains("dependency cycle"));
        let missing = snapshot(vec![record("task", "backlog/tasks/a.md", b"---\nid: TASK-1\nparent: TASK-99\nproject: Undeclared\ndependencies: [TASK-98]\n---\n")]);
        let before = missing.clone();
        validate_storage_snapshot(&missing).unwrap();
        assert_eq!(missing, before);
    }

    #[test]
    fn checked_bind_rolls_back_malformed_native_documents() {
        let root = tempfile::tempdir().unwrap();
        let mut store = Store::open(root.path().join("db")).unwrap();
        let invalid = snapshot(vec![record(
            "task",
            "backlog/tasks/one.md",
            b"---\nid: [invalid\n---\n",
        )]);
        let before = store.change_sequence().unwrap();
        assert!(store
            .bind_exchange_checked(root.path(), &invalid, validate_storage_snapshot)
            .is_err());
        assert!(store.repository(root.path()).unwrap().is_none());
        assert_eq!(store.change_sequence().unwrap(), before);
        let valid = snapshot(vec![task("TASK-1", "", "tasks")]);
        let repo = store
            .bind_exchange_checked(root.path(), &valid, validate_storage_snapshot)
            .unwrap();
        assert_eq!(store.list(&repo, "task").unwrap().len(), 1);
    }
}
