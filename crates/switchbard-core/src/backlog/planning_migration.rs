//! Reviewable, per-repository atomic planning migration with durable backup receipts.
use super::{planning::PlanningState, BacklogTaskSource};
use crate::storage::{Document, RepositoryId, RepositoryLock, Store};
use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

const KINDS: [&str; 3] = ["task", "config", "ranking"];
const CONFIG: &str = "backlog/config.yml";
const RANKING: &str = "backlog/ranking.yml";
const MODERN: [&str; 6] = [
    "Not started",
    "In Progress",
    "Waiting",
    "In Review",
    "Done",
    "Canceled",
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanningTaskChange {
    pub id: String,
    pub from_status: String,
    pub to_status: String,
    pub planning: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanningMigrationPreview {
    pub schema_version: u32,
    pub root: PathBuf,
    pub repository_id: RepositoryId,
    pub epoch_id: String,
    pub task_changes: Vec<PlanningTaskChange>,
    pub unknown_statuses: Vec<String>,
    pub planned_order: Vec<String>,
    pub already_migrated: bool,
    expected: Vec<Document>,
    proposed: Vec<Document>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanningMigrationReceipt {
    pub backup_path: Option<PathBuf>,
    pub receipt_path: Option<PathBuf>,
    pub tasks_changed: usize,
    pub already_applied: bool,
    pub state: String,
    pub before: Vec<Document>,
    pub after: Vec<Document>,
}

fn central(root: &Path) -> Result<(Store, RepositoryId)> {
    let store = Store::open_existing_default()?.context(
        "planning migration requires central task/config/ranking storage; migrate storage first",
    )?;
    let repo = store
        .repository(root)?
        .context("repository is not registered in central storage")?;
    for kind in KINDS {
        ensure!(
            store.authority(&repo, kind)?,
            "planning migration requires central {kind} authority; migrate storage first"
        );
    }
    Ok((store, repo))
}
fn documents(store: &Store, repo: &RepositoryId) -> Result<Vec<Document>> {
    let mut docs = Vec::new();
    for kind in KINDS {
        docs.extend(store.list(repo, kind)?);
    }
    docs.sort_by(|a, b| (&a.kind, &a.locator).cmp(&(&b.kind, &b.locator)));
    for doc in &docs {
        doc.ensure_understood()?;
    }
    Ok(docs)
}

pub fn prepare_planning_migration(root: &Path) -> Result<PlanningMigrationPreview> {
    let root = root.canonicalize()?;
    let _lock = RepositoryLock::acquire(&root)?;
    let (store, repo) = central(&root)?;
    let expected = documents(&store, &repo)?;
    let mut preview = PlanningMigrationPreview {
        schema_version: 2,
        epoch_id: store.export_snapshot(&repo)?.epoch_id,
        root: root.clone(),
        repository_id: repo,
        task_changes: Vec::new(),
        unknown_statuses: Vec::new(),
        planned_order: Vec::new(),
        already_migrated: false,
        proposed: expected.clone(),
        expected,
    };
    plan_tasks(&root, &mut preview)?;
    plan_aggregates(&mut preview)?;
    ensure!(
        documents(&store, &preview.repository_id)? == preview.expected,
        "records changed while preparing migration; retry"
    );
    preview.already_migrated = preview.expected == preview.proposed;
    Ok(preview)
}

fn plan_tasks(root: &Path, preview: &mut PlanningMigrationPreview) -> Result<()> {
    let repo = super::load_backlog_repo(root)?;
    let unique: std::collections::HashSet<_> = repo
        .tasks
        .iter()
        .map(|task| task.id.to_ascii_uppercase())
        .collect();
    ensure!(
        unique.len() == repo.tasks.len(),
        "duplicate task identities; resolve before migration"
    );
    ensure!(
        repo.tasks.len()
            == preview
                .expected
                .iter()
                .filter(|doc| doc.kind == "task" && !doc.deleted)
                .count(),
        "some task records could not be parsed; resolve warnings before migration: {}",
        repo.warnings.join("; ")
    );
    for task in &repo.tasks {
        let locator = task
            .path
            .strip_prefix(root)?
            .to_str()
            .context("invalid task path")?;
        let doc = preview
            .proposed
            .iter_mut()
            .find(|doc| doc.kind == "task" && doc.locator == locator)
            .context("task snapshot missing")?;
        let text = std::str::from_utf8(&doc.content)?;
        let (mapping, _) = super::parse::split_frontmatter(text);
        let raw_status = mapping
            .get(serde_yaml::Value::String("status".into()))
            .and_then(|value| value.as_str())
            .unwrap_or(&task.status);
        let explicit = mapping.get(serde_yaml::Value::String("planning".into()));
        let planning = match explicit {
            Some(value) => value
                .as_str()
                .context("planning must be a string")?
                .parse::<PlanningState>()?,
            None => PlanningState::legacy(raw_status, task.source),
        };
        let status = mapped_status(raw_status, task.source);
        if !known_status(raw_status) {
            preview.unknown_statuses.push(raw_status.into());
        }
        let (updated, _) = super::write::edit_text(text, |draft| {
            if explicit.is_none() {
                let _outcome = super::write::set_task_planning_draft(draft, planning)?;
            }
            if status != raw_status {
                let _outcome = super::write::set_task_status_draft(draft, &status)?;
            }
            Ok(())
        })?;
        let updated = retain_update_timestamp(text, &updated);
        if updated.as_bytes() != doc.content {
            preview.task_changes.push(PlanningTaskChange {
                id: task.id.clone(),
                from_status: raw_status.into(),
                to_status: status.clone(),
                planning: planning.as_str().into(),
            });
        }
        doc.content = updated.into_bytes();
        if task.source == BacklogTaskSource::Active
            && !task.is_done()
            && !["Canceled", "Cancelled", "Archived"]
                .iter()
                .any(|s| status.eq_ignore_ascii_case(s))
            && planning == PlanningState::Planned
        {
            preview.planned_order.push(task.id.clone());
        }
    }
    if let Some(stored) = &repo.ranking.planned {
        let eligible = preview.planned_order.clone();
        preview.planned_order = Vec::new();
        for id in stored {
            if eligible.contains(id) && !preview.planned_order.contains(id) {
                preview.planned_order.push(id.clone());
            }
        }
        for id in eligible {
            if !preview.planned_order.contains(&id) {
                preview.planned_order.push(id);
            }
        }
    }
    preview.unknown_statuses.sort();
    preview.unknown_statuses.dedup();
    Ok(())
}
fn retain_update_timestamp(original: &str, updated: &str) -> String {
    let original_line = original
        .split_inclusive('\n')
        .skip(1)
        .take_while(|line| line.trim() != "---")
        .find(|line| line.starts_with("updated_date:"));
    let mut in_frontmatter = true;
    let mut output = String::new();
    for (index, line) in updated.split_inclusive('\n').enumerate() {
        if index > 0 && line.trim() == "---" {
            in_frontmatter = false;
        }
        if in_frontmatter && line.starts_with("updated_date:") {
            if let Some(original_line) = original_line {
                output.push_str(original_line);
            }
        } else {
            output.push_str(line);
        }
    }
    output
}

fn mapped_status(status: &str, source: BacklogTaskSource) -> String {
    if matches!(
        source,
        BacklogTaskSource::Completed | BacklogTaskSource::Archived
    ) {
        return status.into();
    }
    if ["Icebox", "Backlog", "To Do"]
        .iter()
        .any(|s| s.eq_ignore_ascii_case(status))
    {
        "Not started".into()
    } else {
        status.into()
    }
}
fn known_status(status: &str) -> bool {
    MODERN
        .iter()
        .chain(["Icebox", "Backlog", "To Do", "Archived"].iter())
        .any(|s| s.eq_ignore_ascii_case(status))
}

fn plan_aggregates(preview: &mut PlanningMigrationPreview) -> Result<()> {
    let config = preview
        .proposed
        .iter()
        .find(|doc| doc.kind == "config" && doc.locator == CONFIG && !doc.deleted);
    let text = config
        .map(|doc| std::str::from_utf8(&doc.content))
        .transpose()?
        .unwrap_or("");
    let configured: serde_yaml::Value = serde_yaml::from_str(text)?;
    let mut states: Vec<String> = MODERN.iter().map(|v| (*v).into()).collect();
    if let Some(values) = configured.get("statuses").and_then(|v| v.as_sequence()) {
        for value in values {
            let state = value.as_str().context("status must be a string")?;
            if !known_status(state) && !states.iter().any(|s| s == state) {
                states.push(state.into());
                preview.unknown_statuses.push(state.into());
            }
        }
    }
    for state in &preview.unknown_statuses {
        if !states.contains(state) {
            states.push(state.clone());
        }
    }
    preview.unknown_statuses.sort();
    preview.unknown_statuses.dedup();
    let config = replace_default_status(&replace_statuses(text, &states)?)?;
    let verified: serde_yaml::Value = serde_yaml::from_str(&config)
        .context("status configuration layout cannot be migrated safely")?;
    ensure!(
        verified.get("statuses") == Some(&serde_yaml::to_value(&states)?),
        "status configuration layout cannot be migrated safely"
    );
    set_document(preview, "config", CONFIG, config.into_bytes());
    let ranking = preview
        .proposed
        .iter()
        .find(|doc| doc.kind == "ranking" && doc.locator == RANKING && !doc.deleted);
    let content = super::ranking::planned_document(
        ranking.map(|doc| doc.content.as_slice()),
        &preview.planned_order,
    )?;
    set_document(preview, "ranking", RANKING, content);
    Ok(())
}
fn set_document(
    preview: &mut PlanningMigrationPreview,
    kind: &str,
    locator: &str,
    content: Vec<u8>,
) {
    if let Some(doc) = preview
        .proposed
        .iter_mut()
        .find(|doc| doc.kind == kind && doc.locator == locator)
    {
        doc.content = content;
        doc.deleted = false;
    } else {
        preview.proposed.push(Document {
            id: String::new(),
            repo_id: preview.repository_id.clone(),
            kind: kind.into(),
            locator: locator.into(),
            revision: 0,
            content,
            content_version: 1,
            deleted: false,
        });
    }
    preview
        .proposed
        .sort_by(|a, b| (&a.kind, &a.locator).cmp(&(&b.kind, &b.locator)));
}

fn replace_statuses(text: &str, states: &[String]) -> Result<String> {
    let rendered = format!("statuses: {}", serde_json::to_string(states)?);
    let mut output = String::new();
    let mut skipping = false;
    let mut replaced = false;
    for line in text.split_inclusive('\n') {
        if line.starts_with("statuses:") {
            ensure!(!replaced, "duplicate statuses key");
            output.push_str(&rendered);
            if let Some(end) = flow_sequence_end(line) {
                output.push_str(&line[end + 1..]);
            } else if line.ends_with('\n') {
                output.push('\n');
            }
            skipping = !line.contains('[');
            replaced = true;
        } else if skipping && line.trim_start().starts_with("- ") {
            // Remove only sequence values, retaining unrelated comments and blank lines.
        } else {
            if !line.trim().is_empty() && !line.trim_start().starts_with('#') {
                skipping = false;
            }
            output.push_str(line);
        }
    }
    if !replaced {
        if !output.is_empty() && !output.ends_with('\n') {
            output.push('\n');
        }
        output.push_str(&rendered);
        output.push('\n');
    }
    Ok(output)
}

/// Find the YAML flow sequence boundary, ignoring quoted brackets and trailing comments.
fn flow_sequence_end(line: &str) -> Option<usize> {
    let mut quote = None;
    let mut escaped = false;
    let mut depth = 0usize;
    let mut chars = line.char_indices().peekable();
    for _ in 0..line.chars().count() {
        let (index, ch) = chars.next()?;
        if let Some(active) = quote {
            if active == '"' && escaped {
                escaped = false;
                continue;
            }
            if active == '"' && ch == '\\' {
                escaped = true;
                continue;
            }
            if ch == active {
                if active == '\'' && chars.peek().is_some_and(|(_, next)| *next == '\'') {
                    chars.next();
                } else {
                    quote = None;
                }
            }
        } else {
            match ch {
                '\'' | '"' => quote = Some(ch),
                '#' if index == 0
                    || line[..index]
                        .chars()
                        .last()
                        .is_some_and(char::is_whitespace) =>
                {
                    return None
                }
                '[' => depth += 1,
                ']' if depth == 1 => return Some(index),
                ']' => depth = depth.saturating_sub(1),
                _ => {}
            }
        }
    }
    None
}

fn replace_default_status(text: &str) -> Result<String> {
    let config: serde_yaml::Value = serde_yaml::from_str(text)?;
    let Some(default) = config
        .get("default_status")
        .and_then(|value| value.as_str())
    else {
        return Ok(text.into());
    };
    if !["To Do", "Icebox", "Backlog"]
        .iter()
        .any(|value| value.eq_ignore_ascii_case(default))
    {
        return Ok(text.into());
    }
    let mut output = String::new();
    for line in text.split_inclusive('\n') {
        if line.starts_with("default_status:") {
            output.push_str("default_status: Not started");
            if let Some(comment) = line.find('#') {
                output.push(' ');
                output.push_str(&line[comment..]);
            } else if line.ends_with('\n') {
                output.push('\n');
            }
        } else {
            output.push_str(line);
        }
    }
    Ok(output)
}

pub fn apply_planning_migration(
    root: &Path,
    preview: &PlanningMigrationPreview,
    backup_dir: &Path,
) -> Result<PlanningMigrationReceipt> {
    ensure!(
        preview.schema_version == 2,
        "unsupported migration preview version"
    );
    ensure!(
        root.canonicalize()? == preview.root,
        "preview belongs to a different repository"
    );
    let _lock = RepositoryLock::acquire(root)?;
    let (mut store, repo) = central(root)?;
    ensure!(repo == preview.repository_id, "repository identity changed");
    ensure!(
        store.export_snapshot(&repo)?.epoch_id == preview.epoch_id,
        "repository epoch changed; prepare migration again"
    );
    let current = documents(&store, &repo)?;
    if same_result(&current, &preview.proposed) {
        return Ok(PlanningMigrationReceipt {
            backup_path: None,
            receipt_path: None,
            tasks_changed: 0,
            already_applied: true,
            state: "already_applied".into(),
            before: current.clone(),
            after: current,
        });
    }
    ensure!(
        current == preview.expected,
        "migration preview is stale; prepare again"
    );
    ensure!(
        prepare_planning_migration(root)? == *preview,
        "migration preview differs from current migration rules; prepare again"
    );
    std::fs::create_dir_all(backup_dir)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(backup_dir, std::fs::Permissions::from_mode(0o700))?;
    }
    let name = format!("planning-{}-{}", repo.0, uuid::Uuid::new_v4());
    let backup_path = backup_dir.join(format!("{name}.sqlite3"));
    let receipt_path = backup_dir.join(format!("{name}.json"));
    store.backup_to(&backup_path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&backup_path, std::fs::Permissions::from_mode(0o600))?;
    }
    let mut receipt = PlanningMigrationReceipt {
        backup_path: Some(backup_path),
        receipt_path: Some(receipt_path.clone()),
        tasks_changed: preview.task_changes.len(),
        already_applied: false,
        state: "prepared".into(),
        before: current,
        after: Vec::new(),
    };
    write_receipt(&receipt_path, &receipt)?;
    store
        .mutate_kinds(&repo, &KINDS, None, |docs, _| {
            docs.sort_by(|a, b| (&a.kind, &a.locator).cmp(&(&b.kind, &b.locator)));
            ensure!(
                *docs == preview.expected,
                "migration preview is stale; no changes applied"
            );
            *docs = preview.proposed.clone();
            Ok(())
        })
        .context("migration did not commit; backup and prepared receipt retained")?;
    receipt.after = documents(&store, &repo)?;
    ensure!(
        same_result(&receipt.after, &preview.proposed),
        "migration committed but readback differs; inspect backup and prepared receipt"
    );
    receipt.state = "applied".into();
    write_receipt(&receipt_path, &receipt).context(
        "migration committed; receipt finalization failed; inspect prepared receipt and backup",
    )?;
    Ok(receipt)
}
fn same_result(actual: &[Document], desired: &[Document]) -> bool {
    actual.len() == desired.len()
        && actual.iter().zip(desired).all(|(a, b)| {
            (b.id.is_empty() || a.id == b.id)
                && a.repo_id == b.repo_id
                && a.kind == b.kind
                && a.locator == b.locator
                && a.content == b.content
                && a.deleted == b.deleted
                && a.content_version == b.content_version
        })
}
fn write_receipt(path: &Path, receipt: &PlanningMigrationReceipt) -> Result<()> {
    let text = serde_json::to_string_pretty(receipt)?;
    use std::io::Write;
    let temporary = path.with_extension("json.tmp");
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&temporary)?;
    file.write_all(text.as_bytes())?;
    file.sync_all()?;
    std::fs::rename(&temporary, path)?;
    std::fs::File::open(path.parent().context("receipt has no parent")?)?.sync_all()?;
    Ok(())
}

#[cfg(test)]
#[path = "planning_migration_tests.rs"]
mod tests;
