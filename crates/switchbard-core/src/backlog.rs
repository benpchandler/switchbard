use anyhow::{anyhow, bail, Context, Result};
use serde_yaml::{Mapping, Value};
use std::cmp::Ordering;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

pub const BACKLOG_STATUSES: &[&str] = &["To Do", "In Progress", "Done"];
pub const BACKLOG_PRIORITIES: &[&str] = &["high", "medium", "low"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BacklogProject {
    pub root: PathBuf,
    pub cli_path: Option<PathBuf>,
    pub tasks: Vec<BacklogTask>,
    pub warnings: Vec<String>,
    pub loaded_at_unix: u64,
}

impl BacklogProject {
    pub fn cli_available(&self) -> bool {
        self.cli_path.is_some()
    }

    pub fn active_task_count(&self) -> usize {
        self.tasks
            .iter()
            .filter(|task| task.source == BacklogTaskSource::Active)
            .count()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BacklogTaskSource {
    Active,
    Completed,
    Draft,
    Archived,
}

impl BacklogTaskSource {
    pub fn label(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Completed => "completed",
            Self::Draft => "draft",
            Self::Archived => "archived",
        }
    }

    fn editable(self) -> bool {
        matches!(self, Self::Active)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BacklogTask {
    pub id: String,
    pub title: String,
    pub status: String,
    pub priority: String,
    pub assignees: Vec<String>,
    pub labels: Vec<String>,
    pub dependencies: Vec<String>,
    pub references: Vec<String>,
    pub milestone: Option<String>,
    pub parent: Option<String>,
    pub created_date: Option<String>,
    pub updated_date: Option<String>,
    pub description: String,
    pub implementation_plan: String,
    pub implementation_notes: String,
    pub final_summary: String,
    pub acceptance_criteria: Vec<BacklogChecklistItem>,
    pub definition_of_done: Vec<BacklogChecklistItem>,
    pub source: BacklogTaskSource,
    pub path: PathBuf,
}

impl BacklogTask {
    pub fn editable(&self) -> bool {
        self.source.editable()
    }

    pub fn acceptance_done_count(&self) -> usize {
        self.acceptance_criteria
            .iter()
            .filter(|item| item.checked)
            .count()
    }

    pub fn dod_done_count(&self) -> usize {
        self.definition_of_done
            .iter()
            .filter(|item| item.checked)
            .count()
    }

    /// `true` for the statuses the burndown/statistics views treat as
    /// finished. Mirrors `sort::task_is_completed`'s GUI-side notion but
    /// lives in core so `backlog_stats` (which has no GUI dependency) can
    /// share the exact same definition rather than re-deriving it.
    pub fn is_done(&self) -> bool {
        self.source == BacklogTaskSource::Completed || self.status.eq_ignore_ascii_case("done")
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BacklogChecklistItem {
    pub index: usize,
    pub checked: bool,
    pub text: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BacklogTaskPatch {
    pub title: Option<String>,
    pub description: Option<String>,
    pub status: Option<String>,
    pub priority: Option<String>,
    pub labels: Option<Vec<String>>,
    pub assignees: Option<Vec<String>>,
    pub dependencies: Option<Vec<String>>,
    /// `--ref` replaces the whole references list per invocation (verified
    /// against the live CLI — it is a set operation, not additive), so
    /// "adding" a reference from the UI means submitting the full list with
    /// the new entry appended, same shape as `labels`/`dependencies`.
    pub references: Option<Vec<String>>,
    pub implementation_plan: Option<String>,
    /// `Some(name)` assigns the milestone; `None` with `clear_milestone` unset
    /// leaves it untouched. Assign and clear are mutually exclusive — callers
    /// that want to clear set `clear_milestone` instead of this field.
    pub milestone: Option<String>,
    /// Clears the task's milestone assignment (`--clear-milestone`). Ignored
    /// if `milestone` is also set (assigning wins) — `is_empty` doesn't need
    /// to police that; `edit_backlog_task` only ever receives one or the
    /// other from the UI layer.
    pub clear_milestone: bool,
}

impl BacklogTaskPatch {
    pub fn is_empty(&self) -> bool {
        self.title.is_none()
            && self.description.is_none()
            && self.status.is_none()
            && self.priority.is_none()
            && self.labels.is_none()
            && self.assignees.is_none()
            && self.dependencies.is_none()
            && self.references.is_none()
            && self.implementation_plan.is_none()
            && self.milestone.is_none()
            && !self.clear_milestone
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewBacklogTask {
    pub title: String,
    pub description: String,
    pub status: String,
    pub priority: String,
    pub acceptance_criteria: Vec<String>,
    /// Parent task id (task-17's create-subtask), passed as `-p`/`--parent`.
    pub parent: Option<String>,
    /// QA parity matrix LOW gap: set at creation time via `-l`, same
    /// comma-joined shape `BacklogTaskPatch::labels` uses for edit.
    pub labels: Vec<String>,
    /// Passed as `-a`, comma-joined (verified against `backlog task create
    /// --help`, same flag `edit_backlog_task` already uses for
    /// `BacklogTaskPatch::assignees`).
    pub assignees: Vec<String>,
    /// Passed as `-m` (verified against `backlog task create --help`).
    pub milestone: Option<String>,
    /// Passed as `--depends-on`, comma-joined (verified against `backlog
    /// task create --help`; same flag `edit_backlog_task` uses for
    /// `BacklogTaskPatch::dependencies`).
    pub dependencies: Vec<String>,
}

pub fn is_backlog_project(root: &Path) -> bool {
    root.join("backlog/config.yml").is_file()
        || root.join("backlog/tasks").is_dir()
        || root.join("backlog/drafts").is_dir()
}

pub fn backlog_cli_path() -> Option<PathBuf> {
    find_on_path("backlog").or_else(|| {
        ["/opt/homebrew/bin/backlog", "/usr/local/bin/backlog"]
            .into_iter()
            .map(PathBuf::from)
            .find(|path| path.is_file())
    })
}

pub fn load_backlog_project(root: &Path) -> Result<BacklogProject> {
    if !is_backlog_project(root) {
        bail!("{} is not a Backlog project", root.display());
    }

    let cli_path = backlog_cli_path();
    let mut warnings = Vec::new();
    if cli_path.is_none() {
        warnings.push("Backlog CLI not found on PATH".to_string());
    }

    let mut tasks = Vec::new();
    for (rel, source) in [
        ("tasks", BacklogTaskSource::Active),
        ("completed", BacklogTaskSource::Completed),
        ("drafts", BacklogTaskSource::Draft),
        ("archive/tasks", BacklogTaskSource::Archived),
    ] {
        let dir = root.join("backlog").join(rel);
        if !dir.is_dir() {
            continue;
        }
        let mut entries = fs::read_dir(&dir)
            .with_context(|| format!("cannot read {}", dir.display()))?
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| path.extension().and_then(OsStr::to_str) == Some("md"))
            .collect::<Vec<_>>();
        entries.sort();
        for path in entries {
            match parse_task_file(&path, source) {
                Ok(task) => tasks.push(task),
                Err(err) => warnings.push(format!("{}: {err}", path.display())),
            }
        }
    }

    tasks.sort_by(compare_tasks);
    Ok(BacklogProject {
        root: root.to_path_buf(),
        cli_path,
        tasks,
        warnings,
        loaded_at_unix: unix_now(),
    })
}

pub fn edit_backlog_task(
    project_root: &Path,
    task_id: &str,
    patch: &BacklogTaskPatch,
) -> Result<String> {
    if patch.is_empty() {
        return Ok("no changes".to_string());
    }
    let mut args: Vec<OsString> = vec![
        "task".into(),
        "edit".into(),
        task_id.into(),
        "--plain".into(),
    ];
    if let Some(title) = &patch.title {
        args.push("-t".into());
        args.push(title.into());
    }
    if let Some(description) = &patch.description {
        args.push("-d".into());
        args.push(description.into());
    }
    if let Some(status) = &patch.status {
        args.push("-s".into());
        args.push(status.into());
    }
    if let Some(priority) = &patch.priority {
        args.push("--priority".into());
        args.push(priority.into());
    }
    if let Some(labels) = &patch.labels {
        args.push("-l".into());
        args.push(labels.join(",").into());
    }
    if let Some(assignees) = &patch.assignees {
        args.push("-a".into());
        args.push(assignees.join(",").into());
    }
    if let Some(dependencies) = &patch.dependencies {
        args.push("--depends-on".into());
        args.push(dependencies.join(",").into());
    }
    if let Some(references) = &patch.references {
        for reference in references {
            args.push("--ref".into());
            args.push(reference.into());
        }
    }
    if let Some(plan) = &patch.implementation_plan {
        args.push("--plan".into());
        args.push(plan.into());
    }
    if let Some(milestone) = &patch.milestone {
        args.push("-m".into());
        args.push(milestone.into());
    } else if patch.clear_milestone {
        args.push("--clear-milestone".into());
    }
    run_backlog(project_root, args)
}

pub fn set_backlog_dod_checked(
    project_root: &Path,
    task_id: &str,
    index: usize,
    checked: bool,
) -> Result<String> {
    let flag = if checked {
        "--check-dod"
    } else {
        "--uncheck-dod"
    };
    run_backlog(
        project_root,
        [
            OsString::from("task"),
            OsString::from("edit"),
            OsString::from(task_id),
            OsString::from("--plain"),
            OsString::from(flag),
            OsString::from(index.to_string()),
        ],
    )
}

pub fn archive_backlog_task(project_root: &Path, task_id: &str) -> Result<String> {
    run_backlog(
        project_root,
        [
            OsString::from("task"),
            OsString::from("archive"),
            OsString::from(task_id),
        ],
    )
}

/// Backlog.md's terminal disposition for a *Done* task (verified against a
/// real fixture repo: `backlog task complete --help` and the CLI's own
/// refusal of `task archive` on a Done task — "Done tasks should be
/// completed, not archived. Use: backlog task complete"). Moves the task
/// into `backlog/completed/`, reparsed as `BacklogTaskSource::Completed`,
/// not `Archived`. `archive_backlog_task` (above) is for a non-Done task's
/// abandonment and stays unchanged; the two are mutually exclusive
/// destinations for a task, chosen by status, not interchangeable options.
/// No `--plain` flag — same as `archive_backlog_task`, confirmed via
/// `backlog task complete --help`, which lists none.
pub fn complete_backlog_task(project_root: &Path, task_id: &str) -> Result<String> {
    run_backlog(
        project_root,
        [
            OsString::from("task"),
            OsString::from("complete"),
            OsString::from(task_id),
        ],
    )
}

pub fn set_backlog_acceptance_checked(
    project_root: &Path,
    task_id: &str,
    index: usize,
    checked: bool,
) -> Result<String> {
    let flag = if checked {
        "--check-ac"
    } else {
        "--uncheck-ac"
    };
    run_backlog(
        project_root,
        [
            OsString::from("task"),
            OsString::from("edit"),
            OsString::from(task_id),
            OsString::from("--plain"),
            OsString::from(flag),
            OsString::from(index.to_string()),
        ],
    )
}

/// Atomically swap one label for another via the CLI's own `--remove-label`/
/// `--add-label` flags (a single `task edit` invocation), rather than
/// round-tripping the full label list through `-l` — that would race with any
/// other label a human or another process added between our read and write.
/// This is the in-flight guard the dispatch queue depends on: a task moves
/// `dispatch` → `dispatching` before work starts, so a queue reload never
/// sees it as eligible twice.
pub fn swap_backlog_label(
    project_root: &Path,
    task_id: &str,
    from: &str,
    to: &str,
) -> Result<String> {
    run_backlog(
        project_root,
        [
            OsString::from("task"),
            OsString::from("edit"),
            OsString::from(task_id),
            OsString::from("--plain"),
            OsString::from("--remove-label"),
            OsString::from(from),
            OsString::from("--add-label"),
            OsString::from(to),
        ],
    )
}

/// Add or remove one label without touching a task's other labels — the
/// per-task "Dispatch" opt-in affordance uses this to flag/unflag
/// `dispatch::DISPATCH_LABEL` rather than a full `BacklogTaskPatch::labels`
/// replace, which would race a concurrent edit of some other label (e.g. the
/// dispatch worker's own `swap_backlog_label` running at the same moment).
pub fn set_backlog_label(
    project_root: &Path,
    task_id: &str,
    label: &str,
    enabled: bool,
) -> Result<String> {
    let flag = if enabled {
        "--add-label"
    } else {
        "--remove-label"
    };
    run_backlog(
        project_root,
        [
            OsString::from("task"),
            OsString::from("edit"),
            OsString::from(task_id),
            OsString::from("--plain"),
            OsString::from(flag),
            OsString::from(label),
        ],
    )
}

pub fn append_backlog_notes(project_root: &Path, task_id: &str, note: &str) -> Result<String> {
    if note.trim().is_empty() {
        bail!("note is empty");
    }
    run_backlog(
        project_root,
        [
            OsString::from("task"),
            OsString::from("edit"),
            OsString::from(task_id),
            OsString::from("--plain"),
            OsString::from("--append-notes"),
            OsString::from(note),
        ],
    )
}

pub fn create_backlog_task(project_root: &Path, task: &NewBacklogTask) -> Result<String> {
    if task.title.trim().is_empty() {
        bail!("title is required");
    }
    let mut args: Vec<OsString> = vec![
        "task".into(),
        "create".into(),
        task.title.clone().into(),
        "--plain".into(),
    ];
    if !task.description.trim().is_empty() {
        args.push("-d".into());
        args.push(task.description.clone().into());
    }
    if !task.status.trim().is_empty() {
        args.push("-s".into());
        args.push(task.status.clone().into());
    }
    if !task.priority.trim().is_empty() {
        args.push("--priority".into());
        args.push(task.priority.clone().into());
    }
    for criterion in &task.acceptance_criteria {
        if criterion.trim().is_empty() {
            continue;
        }
        args.push("--ac".into());
        args.push(criterion.clone().into());
    }
    if let Some(parent) = &task.parent {
        args.push("-p".into());
        args.push(parent.clone().into());
    }
    if !task.labels.is_empty() {
        args.push("-l".into());
        args.push(task.labels.join(",").into());
    }
    if !task.assignees.is_empty() {
        args.push("-a".into());
        args.push(task.assignees.join(",").into());
    }
    if let Some(milestone) = &task.milestone {
        args.push("-m".into());
        args.push(milestone.clone().into());
    }
    if !task.dependencies.is_empty() {
        args.push("--depends-on".into());
        args.push(task.dependencies.join(",").into());
    }
    run_backlog(project_root, args)
}

fn run_backlog<I, S>(project_root: &Path, args: I) -> Result<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let cli = backlog_cli_path().ok_or_else(|| {
        anyhow!(
            "Backlog CLI not found. Install backlog or make it visible on PATH before editing tasks."
        )
    })?;
    let output = Command::new(&cli)
        .current_dir(project_root)
        .args(args)
        .output()
        .with_context(|| format!("failed to run {}", cli.display()))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let msg = if stderr.is_empty() { stdout } else { stderr };
        bail!("backlog failed: {msg}");
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn find_on_path(name: &str) -> Option<PathBuf> {
    let path_var = std::env::var_os("PATH")?;
    std::env::split_paths(&path_var)
        .map(|dir| dir.join(name))
        .find(|path| path.is_file())
}

fn parse_task_file(path: &Path, source: BacklogTaskSource) -> Result<BacklogTask> {
    let text = fs::read_to_string(path).with_context(|| "cannot read task markdown")?;
    let (frontmatter, body) = split_frontmatter(&text);
    let id = yaml_string(&frontmatter, "id").unwrap_or_else(|| id_from_filename(path));
    let title = yaml_string(&frontmatter, "title").unwrap_or_else(|| id.clone());
    let status = yaml_string(&frontmatter, "status").unwrap_or_else(|| match source {
        BacklogTaskSource::Completed => "Done".to_string(),
        BacklogTaskSource::Draft => "Draft".to_string(),
        BacklogTaskSource::Archived => "Archived".to_string(),
        BacklogTaskSource::Active => "To Do".to_string(),
    });
    let priority = yaml_string(&frontmatter, "priority").unwrap_or_else(|| "medium".to_string());
    let description = extract_section(body, "Description");
    let implementation_plan = extract_section(body, "Implementation Plan");
    let implementation_notes = extract_section(body, "Implementation Notes");
    let final_summary = extract_section(body, "Final Summary");
    let acceptance_criteria =
        parse_checklist_section(&extract_section(body, "Acceptance Criteria"));
    let definition_of_done = parse_checklist_section(&extract_section(body, "Definition of Done"));

    Ok(BacklogTask {
        id,
        title,
        status,
        priority,
        assignees: yaml_string_list(&frontmatter, "assignee"),
        labels: yaml_string_list(&frontmatter, "labels"),
        dependencies: yaml_string_list(&frontmatter, "dependencies"),
        references: yaml_string_list(&frontmatter, "references"),
        milestone: yaml_string(&frontmatter, "milestone"),
        // The real `backlog` CLI (v1.47.1) writes `parent_task_id:`, not
        // `parent:` — confirmed empirically in the 2026-08-05 QA audit
        // (docs/qa/2026-08-05-parity-qa.md, Defect 1). Fall back to the old
        // key so fixtures/tasks written before this fix still parse.
        parent: yaml_string(&frontmatter, "parent_task_id")
            .or_else(|| yaml_string(&frontmatter, "parent")),
        created_date: yaml_string(&frontmatter, "created_date"),
        updated_date: yaml_string(&frontmatter, "updated_date"),
        description,
        implementation_plan,
        implementation_notes,
        final_summary,
        acceptance_criteria,
        definition_of_done,
        source,
        path: path.to_path_buf(),
    })
}

fn split_frontmatter(text: &str) -> (Mapping, &str) {
    let Some(rest) = text.strip_prefix("---") else {
        return (Mapping::new(), text);
    };
    let rest = rest.strip_prefix('\n').unwrap_or(rest);
    let Some(end) = rest.find("\n---") else {
        return (Mapping::new(), text);
    };
    let yaml_text = &rest[..end];
    let body_start = end + "\n---".len();
    let body = rest[body_start..]
        .strip_prefix('\n')
        .unwrap_or(&rest[body_start..]);
    let mapping = serde_yaml::from_str::<Value>(yaml_text)
        .ok()
        .and_then(|value| value.as_mapping().cloned())
        .unwrap_or_default();
    (mapping, body)
}

fn yaml_string(map: &Mapping, key: &str) -> Option<String> {
    let value = map.get(Value::String(key.to_string()))?;
    match value {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        _ => None,
    }
    .map(|s| s.trim().to_string())
    .filter(|s| !s.is_empty())
}

fn yaml_string_list(map: &Mapping, key: &str) -> Vec<String> {
    let Some(value) = map.get(Value::String(key.to_string())) else {
        return Vec::new();
    };
    match value {
        Value::Sequence(items) => items
            .iter()
            .filter_map(|item| match item {
                Value::String(s) => Some(s.trim().to_string()),
                Value::Number(n) => Some(n.to_string()),
                Value::Bool(b) => Some(b.to_string()),
                _ => None,
            })
            .filter(|s| !s.is_empty())
            .collect(),
        Value::String(s) => s
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .collect(),
        _ => Vec::new(),
    }
}

fn extract_section(body: &str, heading: &str) -> String {
    let mut in_section = false;
    let mut lines = Vec::new();
    for line in body.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("## ") {
            if in_section {
                break;
            }
            let title = trimmed.trim_start_matches('#').trim();
            if title.eq_ignore_ascii_case(heading) {
                in_section = true;
            }
            continue;
        }
        if in_section && !trimmed.starts_with("<!--") {
            lines.push(line);
        }
    }
    lines.join("\n").trim().to_string()
}

fn parse_checklist_section(section: &str) -> Vec<BacklogChecklistItem> {
    let mut out = Vec::new();
    for line in section.lines() {
        let Some(rest) = line.trim().strip_prefix("- [") else {
            continue;
        };
        let Some((mark, rest)) = rest.split_once(']') else {
            continue;
        };
        let checked = mark.trim().eq_ignore_ascii_case("x");
        let rest = rest.trim();
        let (index, text) = parse_checklist_index(rest, out.len() + 1);
        if text.is_empty() {
            continue;
        }
        out.push(BacklogChecklistItem {
            index,
            checked,
            text,
        });
    }
    out
}

fn parse_checklist_index(text: &str, fallback: usize) -> (usize, String) {
    let Some(rest) = text.strip_prefix('#') else {
        return (fallback, text.trim().to_string());
    };
    let digits_len = rest
        .chars()
        .take_while(|ch| ch.is_ascii_digit())
        .map(char::len_utf8)
        .sum::<usize>();
    if digits_len == 0 {
        return (fallback, text.trim().to_string());
    }
    let index = rest[..digits_len].parse::<usize>().unwrap_or(fallback);
    let label = rest[digits_len..].trim().to_string();
    (index, label)
}

fn id_from_filename(path: &Path) -> String {
    let stem = path.file_stem().and_then(OsStr::to_str).unwrap_or("task");
    let id = stem
        .split_whitespace()
        .next()
        .unwrap_or(stem)
        .trim_start_matches("task-")
        .trim_start_matches("TASK-");
    format!("TASK-{}", id.to_ascii_uppercase())
}

fn compare_tasks(a: &BacklogTask, b: &BacklogTask) -> Ordering {
    source_rank(a.source)
        .cmp(&source_rank(b.source))
        .then_with(|| status_rank(&a.status).cmp(&status_rank(&b.status)))
        .then_with(|| priority_rank(&a.priority).cmp(&priority_rank(&b.priority)))
        .then_with(|| task_id_key(&a.id).cmp(&task_id_key(&b.id)))
        .then_with(|| a.title.cmp(&b.title))
}

fn source_rank(source: BacklogTaskSource) -> usize {
    match source {
        BacklogTaskSource::Active => 0,
        BacklogTaskSource::Draft => 1,
        BacklogTaskSource::Completed => 2,
        BacklogTaskSource::Archived => 3,
    }
}

fn status_rank(status: &str) -> usize {
    match status.to_ascii_lowercase().as_str() {
        "in progress" => 0,
        "to do" => 1,
        "done" => 2,
        "draft" => 3,
        "archived" => 4,
        _ => 5,
    }
}

fn priority_rank(priority: &str) -> usize {
    match priority.to_ascii_lowercase().as_str() {
        "high" => 0,
        "medium" => 1,
        "low" => 2,
        _ => 3,
    }
}

fn task_id_key(id: &str) -> Vec<u32> {
    id.trim_start_matches("TASK-")
        .split('.')
        .filter_map(|part| part.parse::<u32>().ok())
        .collect()
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

/// Parse a Backlog `"YYYY-MM-DD HH:MM"` timestamp (`created_date`/
/// `updated_date`) into a day count since the Unix epoch. Shared by
/// `backlog_stats` (burndown, portfolio) and `backlog_relations` ("newly
/// unblocked") — both only need day granularity, unlike `backlog_triage`'s
/// age-based tiebreak, which keeps its own seconds-precision parser.
pub fn parse_backlog_day(value: &str) -> Option<i64> {
    chrono::NaiveDateTime::parse_from_str(value.trim(), "%Y-%m-%d %H:%M")
        .ok()
        .map(|dt| dt.and_utc().timestamp().div_euclid(86_400))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_backlog_task_markdown() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("task-18 - Example.md");
        fs::write(
            &path,
            r#"---
id: TASK-18
title: Example task
status: To Do
assignee:
  - ben
labels:
  - research
dependencies: []
priority: low
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Do the thing.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 First criterion
- [x] #2 Second criterion
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Existing note.
<!-- SECTION:NOTES:END -->
"#,
        )
        .unwrap();

        let task = parse_task_file(&path, BacklogTaskSource::Active).unwrap();

        assert_eq!(task.id, "TASK-18");
        assert_eq!(task.title, "Example task");
        assert_eq!(task.priority, "low");
        assert_eq!(task.assignees, vec!["ben"]);
        assert_eq!(task.labels, vec!["research"]);
        assert_eq!(task.description, "Do the thing.");
        assert_eq!(task.implementation_notes, "Existing note.");
        assert_eq!(task.acceptance_criteria.len(), 2);
        assert_eq!(task.acceptance_criteria[0].index, 1);
        assert!(!task.acceptance_criteria[0].checked);
        assert!(task.acceptance_criteria[1].checked);
    }

    /// Regression for the 2026-08-05 QA audit's HIGH defect: the real
    /// `backlog` CLI writes `parent_task_id:`, not `parent:`. This is the
    /// fast, in-process complement to `backlog_cli_mutations.rs`'s real-CLI
    /// round trip — it pins the parser's key preference directly, plus the
    /// fallback for `parent:`-only fixtures written before this fix.
    #[test]
    fn parses_parent_task_id_and_falls_back_to_the_old_parent_key() {
        let dir = tempfile::tempdir().unwrap();

        let real_cli_path = dir.path().join("task-2 - Subtask.md");
        fs::write(
            &real_cli_path,
            "---\nid: TASK-2\ntitle: Subtask\nparent_task_id: TASK-1\n---\n",
        )
        .unwrap();
        let real_cli_task = parse_task_file(&real_cli_path, BacklogTaskSource::Active).unwrap();
        assert_eq!(real_cli_task.parent.as_deref(), Some("TASK-1"));

        let old_fixture_path = dir.path().join("task-3 - Old fixture.md");
        fs::write(
            &old_fixture_path,
            "---\nid: TASK-3\ntitle: Old fixture\nparent: TASK-1\n---\n",
        )
        .unwrap();
        let old_fixture_task =
            parse_task_file(&old_fixture_path, BacklogTaskSource::Active).unwrap();
        assert_eq!(old_fixture_task.parent.as_deref(), Some("TASK-1"));
    }

    #[test]
    fn detects_backlog_project() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("backlog/tasks")).unwrap();

        assert!(is_backlog_project(dir.path()));
    }

    #[test]
    fn sorts_task_id_decimals_numerically() {
        let mut ids = ["TASK-150.10", "TASK-2", "TASK-150.2"];
        ids.sort_by_key(|id| task_id_key(id));

        assert_eq!(ids, ["TASK-2", "TASK-150.2", "TASK-150.10"]);
    }
}
