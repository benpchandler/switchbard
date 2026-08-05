//! Turning a Backlog.md project directory (and the raw task markdown inside
//! it) into `super::types` structs. Nothing here shells out — that's
//! `super::mutations`.

use super::types::{BacklogChecklistItem, BacklogProject, BacklogTask, BacklogTaskSource};
use anyhow::{bail, Context, Result};
use serde_yaml::{Mapping, Value};
use std::cmp::Ordering;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

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
        configured_statuses: parse_config_statuses(root),
    })
}

/// Read `backlog/config.yml`'s `statuses:` array — see `BacklogProject::
/// configured_statuses`'s doc for why this is worth a second read alongside
/// the task files themselves. Never fails the whole project load: a
/// missing/unreadable/malformed config just yields an empty list, same as
/// if this function didn't exist.
fn parse_config_statuses(root: &Path) -> Vec<String> {
    let path = root.join("backlog/config.yml");
    let Ok(text) = fs::read_to_string(&path) else {
        return Vec::new();
    };
    let Ok(value) = serde_yaml::from_str::<Value>(&text) else {
        return Vec::new();
    };
    let Some(mapping) = value.as_mapping() else {
        return Vec::new();
    };
    yaml_string_list(mapping, "statuses")
}

/// TASK-28 (owner-found bug): even with `--plain`, `backlog task create`
/// (unlike `task archive`'s one-line `"Archived task TASK-1"`) writes the
/// *entire* newly-created task's rendered form to stdout — file path, a
/// `====` underline, Status/Ordinal/Created, empty Description/Acceptance
/// Criteria/Definition of Done sections. `create_backlog_task`'s caller
/// used to surface that raw multi-line blob as the GUI's action-status
/// message, stretching the top bar into a many-line void. Every other
/// mutation function in this module already discards its own raw stdout
/// and lets the caller build a compact message instead (see
/// `edit_backlog_task`'s callers); this is the one exception, so rather
/// than have the GUI parse a format it doesn't own, this pulls the id out
/// of the one line worth reading — `"Task TASK-1 - Title"` — so the caller
/// can build `"Created {repo}:{id}"` without touching the rest.
/// Empirically confirmed against a real `backlog init` fixture before
/// writing this parser, not guessed. Returns `None` if that line isn't
/// found (a future CLI output-format change) — callers must fall back to a
/// generic message, not panic.
pub fn parse_created_task_id(output: &str) -> Option<String> {
    output
        .lines()
        .find_map(|line| line.strip_prefix("Task "))
        .and_then(|rest| rest.split(" - ").next())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
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

/// Milliseconds, not seconds — see `BacklogProject::loaded_at_unix`'s doc
/// for why the finer precision matters.
fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
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

    /// TASK-25 (owner-requested UX): `configured_statuses` reads
    /// `backlog/config.yml`'s `statuses:` array — budget's own config
    /// declares exactly this set.
    #[test]
    fn parses_config_statuses_from_config_yml() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("backlog")).unwrap();
        fs::write(
            dir.path().join("backlog/config.yml"),
            "project_name: \"Ledger\"\ndefault_status: \"To Do\"\nstatuses: [\"Icebox\", \"To Do\", \"In Progress\", \"In Review\", \"Done\"]\n",
        )
        .unwrap();

        let statuses = parse_config_statuses(dir.path());
        assert_eq!(
            statuses,
            vec!["Icebox", "To Do", "In Progress", "In Review", "Done"]
        );
    }

    /// Missing/malformed config is never fatal — `configured_statuses` just
    /// comes back empty, same as if the function didn't exist. Confirms
    /// both a fully-missing file and a config.yml with no `statuses` key.
    #[test]
    fn missing_or_statusless_config_yields_an_empty_list() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(parse_config_statuses(dir.path()), Vec::<String>::new());

        fs::create_dir_all(dir.path().join("backlog")).unwrap();
        fs::write(
            dir.path().join("backlog/config.yml"),
            "project_name: \"No statuses key\"\n",
        )
        .unwrap();
        assert_eq!(parse_config_statuses(dir.path()), Vec::<String>::new());
    }

    /// TASK-28: pins the real `backlog task create --plain` output shape
    /// (captured empirically against a real fixture repo) so the parser
    /// stays correct even without a CLI round trip for every test.
    #[test]
    fn parses_created_task_id_from_real_cli_output_shape() {
        let output = "File: /tmp/x/backlog/tasks/task-1 - Test task for stdout inspection.md\n\
                       \n\
                       Task TASK-1 - Test task for stdout inspection\n\
                       ==================================================\n\
                       \n\
                       Status: \u{25cb} To Do\n\
                       Ordinal: 1000\n\
                       Created: 2026-08-05 14:59\n\
                       \n\
                       Description:\n\
                       --------------------------------------------------\n\
                       No description provided\n";
        assert_eq!(parse_created_task_id(output), Some("TASK-1".to_string()));
    }

    #[test]
    fn parse_created_task_id_returns_none_on_unrecognized_output() {
        assert_eq!(parse_created_task_id("some unexpected future format"), None);
        assert_eq!(parse_created_task_id(""), None);
    }

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
