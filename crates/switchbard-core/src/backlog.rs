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
}

impl BacklogTaskPatch {
    pub fn is_empty(&self) -> bool {
        self.title.is_none()
            && self.description.is_none()
            && self.status.is_none()
            && self.priority.is_none()
            && self.labels.is_none()
            && self.assignees.is_none()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewBacklogTask {
    pub title: String,
    pub description: String,
    pub status: String,
    pub priority: String,
    pub acceptance_criteria: Vec<String>,
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
    run_backlog(project_root, args)
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
        milestone: yaml_string(&frontmatter, "milestone"),
        parent: yaml_string(&frontmatter, "parent"),
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
