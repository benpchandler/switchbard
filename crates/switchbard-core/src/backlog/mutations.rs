//! Task mutations by (project root, task id) — the caller-facing facade over
//! the native write layer.
//!
//! Until the format fork's TASK-65 swap, every function here shelled out to
//! the `backlog` CLI; now each resolves the task id to its file under
//! `backlog/tasks` and applies [`super::write`]'s surgical edits directly.
//! The signatures are unchanged, so the GUI's `spawn_backlog_*` methods,
//! `crate::dispatch`, and `crate::refine` did not move.
//!
//! # Behavior kept from the CLI (deliberately)
//!
//! - **Status writes are validated against the project's own
//!   `backlog/config.yml`**, with the same `Invalid status: … Valid statuses
//!   are: …` message shape. The status-vocabulary offer flow
//!   (`super::status_config`, `missing_standard_statuses`) is built around
//!   that refusal; accepting anything here would dissolve it by accident.
//!   TASK-68 owns moving this validation deeper into the write layer.
//! - **`archive` refuses a Done task** ("Done tasks should be completed") and
//!   **`complete` requires one** — the two are status-chosen destinations,
//!   not interchangeable (see each function's doc).
//! - **Label add/remove/swap read the file at write time**, not the caller's
//!   snapshot.
//!
//! # Behavior deliberately changed
//!
//! - **`swap_backlog_label` is strict**: it fails when the task doesn't
//!   carry the source label, where the CLI added the target label anyway. A
//!   dispatch claim is a race for a token; see
//!   [`super::write::swap_task_label`].
//! - **`create_backlog_task` returns the new task's id** (`"TASK-42"`)
//!   instead of a rendered blob to scrape (the late `parse_created_task_id`,
//!   TASK-28's scar).

use super::allocate::{create_task_allocating_id, strip_id_prefix};
use super::parse::{
    configured_task_prefix, parse_config_statuses, parse_task_file, DEFAULT_TASK_PREFIX,
};
use super::types::{BacklogTaskPatch, BacklogTaskSource, NewBacklogTask};
use super::write::{
    append_task_acceptance_criteria, append_task_notes, replace_task_section,
    set_task_checklist_item, set_task_label, set_task_list_field, set_task_priority,
    set_task_project, set_task_status, set_task_title, swap_task_label, TaskChecklist,
    TaskListField, TaskSection, WriteOutcome,
};
use anyhow::{bail, Context, Result};
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};

pub fn edit_backlog_task(
    project_root: &Path,
    task_id: &str,
    patch: &BacklogTaskPatch,
) -> Result<String> {
    if patch.is_empty() {
        return Ok("no changes".to_string());
    }
    if let Some(status) = &patch.status {
        validate_status(project_root, status)?;
    }
    let path = resolve_task_file(project_root, task_id)?;
    let changed = apply_patch(&path, patch)?;
    if changed {
        Ok(format!("Edited {task_id}"))
    } else {
        Ok("no changes".to_string())
    }
}

/// One write-layer call per populated patch field, in the field order
/// [`BacklogTaskPatch`] declares. Each call is itself atomic; a failure
/// partway leaves the earlier fields applied — the same partial-application
/// surface a failed CLI invocation had, minus the fields it batched.
fn apply_patch(path: &Path, patch: &BacklogTaskPatch) -> Result<bool> {
    let mut changed = false;
    if let Some(title) = &patch.title {
        changed |= set_task_title(path, title)?.changed();
    }
    if let Some(description) = &patch.description {
        changed |= replace_task_section(path, TaskSection::Description, description)?.changed();
    }
    if let Some(status) = &patch.status {
        changed |= set_task_status(path, status)?.changed();
    }
    if let Some(priority) = &patch.priority {
        changed |= set_task_priority(path, priority)?.changed();
    }
    for (field, values) in [
        (TaskListField::Labels, &patch.labels),
        (TaskListField::Assignee, &patch.assignees),
        (TaskListField::Dependencies, &patch.dependencies),
        (TaskListField::References, &patch.references),
    ] {
        if let Some(values) = values {
            changed |= set_task_list_field(path, field, values)?.changed();
        }
    }
    if let Some(plan) = &patch.implementation_plan {
        changed |= replace_task_section(path, TaskSection::ImplementationPlan, plan)?.changed();
    }
    if !patch.append_acceptance_criteria.is_empty() {
        changed |=
            append_task_acceptance_criteria(path, &patch.append_acceptance_criteria)?.changed();
    }
    if let Some(project) = &patch.project {
        changed |= set_task_project(path, Some(project))?.changed();
    } else if patch.clear_project {
        changed |= set_task_project(path, None)?.changed();
    }
    Ok(changed)
}

pub fn set_backlog_dod_checked(
    project_root: &Path,
    task_id: &str,
    index: usize,
    checked: bool,
) -> Result<String> {
    let path = resolve_task_file(project_root, task_id)?;
    let outcome = set_task_checklist_item(&path, TaskChecklist::DefinitionOfDone, index, checked)?;
    Ok(outcome_message(task_id, outcome))
}

pub fn set_backlog_acceptance_checked(
    project_root: &Path,
    task_id: &str,
    index: usize,
    checked: bool,
) -> Result<String> {
    let path = resolve_task_file(project_root, task_id)?;
    let outcome =
        set_task_checklist_item(&path, TaskChecklist::AcceptanceCriteria, index, checked)?;
    Ok(outcome_message(task_id, outcome))
}

/// Move a non-Done task into `backlog/archive/tasks/` — abandonment, not
/// completion. Mirrors the CLI's refusal: a Done task's terminal disposition
/// is [`complete_backlog_task`] and `backlog/completed/`; the two are
/// status-chosen destinations, never interchangeable options.
pub fn archive_backlog_task(project_root: &Path, task_id: &str) -> Result<String> {
    let path = resolve_task_file(project_root, task_id)?;
    let (task, _) = parse_task_file(&path, BacklogTaskSource::Active)?;
    if task.status.eq_ignore_ascii_case("done") {
        bail!("Done tasks should be completed, not archived");
    }
    move_task_file(&path, &project_root.join("backlog/archive/tasks"))?;
    Ok(format!("Archived task {}", task.id))
}

/// Move a Done task into `backlog/completed/` (reparsed as
/// `BacklogTaskSource::Completed`). Refuses anything not Done, the exact
/// complement of [`archive_backlog_task`]'s refusal.
pub fn complete_backlog_task(project_root: &Path, task_id: &str) -> Result<String> {
    let path = resolve_task_file(project_root, task_id)?;
    let (task, _) = parse_task_file(&path, BacklogTaskSource::Active)?;
    if !task.status.eq_ignore_ascii_case("done") {
        bail!("only a Done task can be completed; archive it instead");
    }
    move_task_file(&path, &project_root.join("backlog/completed"))?;
    Ok(format!("Completed task {}", task.id))
}

/// Atomically swap one label for another in a single write. Strict claim
/// semantics — fails when the task doesn't carry `from`; see the module doc
/// and [`super::write::swap_task_label`] for why this is deliberately
/// stronger than the CLI swap it replaces. This is the in-flight guard the
/// dispatch queue depends on: a task moves `dispatch` → `dispatching` before
/// work starts, so a queue reload never sees it as eligible twice.
pub fn swap_backlog_label(
    project_root: &Path,
    task_id: &str,
    from: &str,
    to: &str,
) -> Result<String> {
    let path = resolve_task_file(project_root, task_id)?;
    let outcome = swap_task_label(&path, from, to)?;
    Ok(outcome_message(task_id, outcome))
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
    let path = resolve_task_file(project_root, task_id)?;
    let outcome = set_task_label(&path, label, enabled)?;
    Ok(outcome_message(task_id, outcome))
}

/// [`set_backlog_label`] with `enabled = false`, reporting whether the label
/// was actually present — `dispatch::dismiss_run` branches on the fact (which
/// run-state labels a task really carried), not on a display message.
pub fn remove_backlog_label(project_root: &Path, task_id: &str, label: &str) -> Result<bool> {
    let path = resolve_task_file(project_root, task_id)?;
    Ok(set_task_label(&path, label, false)?.changed())
}

pub fn append_backlog_notes(project_root: &Path, task_id: &str, note: &str) -> Result<String> {
    let path = resolve_task_file(project_root, task_id)?;
    let outcome = append_task_notes(&path, note)?;
    Ok(outcome_message(task_id, outcome))
}

/// Replace the task's Final Summary section wholesale — the wrap-up field
/// the task lifecycle requires before Done. Not part of
/// [`BacklogTaskPatch`] because no editing surface composes it with other
/// fields; it is written once, at the end, by whoever finishes the task
/// (today: the `sb` CLI).
pub fn set_backlog_final_summary(
    project_root: &Path,
    task_id: &str,
    summary: &str,
) -> Result<String> {
    if summary.trim().is_empty() {
        bail!("final summary is empty");
    }
    let path = resolve_task_file(project_root, task_id)?;
    let outcome = replace_task_section(&path, TaskSection::FinalSummary, summary)?;
    Ok(outcome_message(task_id, outcome))
}

/// Create a task in `backlog/tasks`, allocating its id natively (see
/// `super::allocate`). Returns the new id — `"TASK-42"`, or `"LED-42"` in a
/// project whose `backlog/config.yml` declares `task_prefix: "LED"` — as the
/// output string; callers build their own status messages from it.
pub fn create_backlog_task(project_root: &Path, task: &NewBacklogTask) -> Result<String> {
    if task.title.trim().is_empty() {
        bail!("title is required");
    }
    if let Some(status) = Some(task.status.as_str()).filter(|s| !s.trim().is_empty()) {
        validate_status(project_root, status)?;
    }
    let prefix = configured_task_prefix(project_root);
    let (id, _path) = create_task_allocating_id(project_root, task)?;
    Ok(format!("{prefix}-{id}"))
}

// ---- shared plumbing ----

/// The same two message shapes `edit_backlog_task` returns, so every
/// mutation reports honestly whether it changed anything.
fn outcome_message(task_id: &str, outcome: WriteOutcome) -> String {
    if outcome.changed() {
        format!("Edited {task_id}")
    } else {
        "no changes".to_string()
    }
}

/// The file in `backlog/tasks` carrying `task_id`. Matches on the id portion
/// of the filename (`{prefix}-{id} - Title.md`, for the project's configured
/// `task_prefix` — `LED-` for budget), case-insensitively, accepting the id
/// with or without that prefix (or the literal `TASK-`, tolerated the same
/// way `super::allocate` tolerates it when scanning — see that module's
/// *Configured id prefix* doc). Zero matches and multiple matches are both
/// errors — a duplicated id is a fact to surface, never to guess through.
fn resolve_task_file(project_root: &Path, task_id: &str) -> Result<PathBuf> {
    let prefix = configured_task_prefix(project_root);
    let key = normalized_id(task_id, &prefix);
    if key.is_empty() {
        bail!("task id is empty");
    }
    let tasks_dir = project_root.join("backlog/tasks");
    let entries =
        fs::read_dir(&tasks_dir).with_context(|| format!("reading {}", tasks_dir.display()))?;
    let mut matches: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(OsStr::to_str) == Some("md"))
        .filter(|path| filename_matches_id(path, key, &prefix))
        .collect();
    matches.sort();
    match matches.len() {
        0 => bail!("no task {task_id} in {}", tasks_dir.display()),
        1 => Ok(matches.remove(0)),
        n => bail!(
            "{n} files in {} carry task id {task_id} — resolve the duplicate before editing",
            tasks_dir.display()
        ),
    }
}

fn normalized_id<'a>(task_id: &'a str, prefix: &str) -> &'a str {
    let trimmed = task_id.trim();
    strip_id_prefix(trimmed, prefix)
        .or_else(|| strip_id_prefix(trimmed, DEFAULT_TASK_PREFIX))
        .unwrap_or(trimmed)
}

fn filename_matches_id(path: &Path, key: &str, prefix: &str) -> bool {
    let Some(stem) = path.file_stem().and_then(OsStr::to_str) else {
        return false;
    };
    let Some(rest) =
        strip_id_prefix(stem, prefix).or_else(|| strip_id_prefix(stem, DEFAULT_TASK_PREFIX))
    else {
        return false;
    };
    let id_part = rest.split_whitespace().next().unwrap_or(rest);
    id_part.eq_ignore_ascii_case(key)
}

/// Same `Invalid status:` message shape the `backlog` CLI produced, so the
/// GUI surfaces and the dispatch pipeline's best-effort status writes see
/// the failure class they were built around. A project declaring no
/// statuses (missing or minimal `config.yml`) constrains nothing.
fn validate_status(project_root: &Path, status: &str) -> Result<()> {
    let declared = parse_config_statuses(project_root);
    if declared.is_empty() || declared.iter().any(|s| s.eq_ignore_ascii_case(status)) {
        return Ok(());
    }
    bail!(
        "Invalid status: {status}. Valid statuses are: {}",
        declared.join(", ")
    )
}

fn move_task_file(from: &Path, dest_dir: &Path) -> Result<()> {
    fs::create_dir_all(dest_dir).with_context(|| format!("creating {}", dest_dir.display()))?;
    let name = from.file_name().context("task path has no filename")?;
    let dest = dest_dir.join(name);
    if dest.exists() {
        bail!("{} already exists", dest.display());
    }
    fs::rename(from, &dest)
        .with_context(|| format!("moving {} to {}", from.display(), dest.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project_with_task(filename: &str) -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("tempdir");
        let tasks = dir.path().join("backlog/tasks");
        fs::create_dir_all(&tasks).expect("fixture dirs");
        fs::write(
            tasks.join(filename),
            "---\nid: TASK-7\ntitle: Fixture\nstatus: To Do\npriority: low\n---\n",
        )
        .expect("fixture file");
        dir
    }

    fn new_task(title: &str) -> NewBacklogTask {
        NewBacklogTask {
            title: title.to_string(),
            description: String::new(),
            status: String::new(),
            priority: String::new(),
            acceptance_criteria: vec![],
            parent: None,
            labels: vec![],
            assignees: vec![],
            project: None,
            dependencies: vec![],
        }
    }

    /// The facade-level slice of the reproduction: `create_backlog_task`'s
    /// returned id string must carry the project's configured prefix, not a
    /// hardcoded `TASK-`.
    #[test]
    fn create_backlog_task_honors_a_configured_prefix() {
        let dir = tempfile::tempdir().expect("tempdir");
        fs::create_dir_all(dir.path().join("backlog")).expect("fixture dirs");
        fs::write(
            dir.path().join("backlog/config.yml"),
            "project_name: \"Fixture\"\ntask_prefix: \"LED\"\n",
        )
        .expect("fixture config");

        let id = create_backlog_task(dir.path(), &new_task("Fix the prefix bug")).expect("creates");

        assert_eq!(id, "LED-1");
    }

    #[test]
    fn resolves_ids_with_and_without_prefix_case_insensitively() {
        let dir = project_with_task("task-7 - Fixture.md");
        for id in ["TASK-7", "task-7", "7", " TASK-7 "] {
            let path = resolve_task_file(dir.path(), id)
                .unwrap_or_else(|e| panic!("{id} should resolve: {e}"));
            assert!(path.ends_with("task-7 - Fixture.md"));
        }
        assert!(
            resolve_task_file(dir.path(), "TASK-70").is_err(),
            "no prefix match"
        );
        assert!(resolve_task_file(dir.path(), "").is_err());
    }

    /// The related bug this fix also closes: `edit`/`archive`/`complete`/etc
    /// all resolve their target file through `resolve_task_file`, which used
    /// to require a literal `task-` filename prefix regardless of the
    /// project's configured `task_prefix` — so `sb edit LED-11`
    /// failed with "no task LED-11" even though the file existed and `view`/
    /// `list` (which read frontmatter directly) found it fine. Reproduced
    /// live against a scratch LED-prefixed project before this fix.
    #[test]
    fn resolves_ids_in_a_led_prefixed_project() {
        let dir = tempfile::tempdir().expect("tempdir");
        fs::create_dir_all(dir.path().join("backlog")).expect("fixture dirs");
        fs::write(
            dir.path().join("backlog/config.yml"),
            "project_name: \"Fixture\"\ntask_prefix: \"LED\"\n",
        )
        .expect("fixture config");
        let tasks = dir.path().join("backlog/tasks");
        fs::create_dir_all(&tasks).expect("fixture dirs");
        fs::write(
            tasks.join("led-11 - Fixture.md"),
            "---\nid: LED-11\ntitle: Fixture\nstatus: To Do\npriority: low\n---\n",
        )
        .expect("fixture file");

        for id in ["LED-11", "led-11", "11", " LED-11 "] {
            let path = resolve_task_file(dir.path(), id)
                .unwrap_or_else(|e| panic!("{id} should resolve: {e}"));
            assert!(path.ends_with("led-11 - Fixture.md"));
        }
    }

    #[test]
    fn a_duplicated_id_is_an_error_not_a_guess() {
        let dir = project_with_task("task-7 - Fixture.md");
        fs::write(
            dir.path().join("backlog/tasks/task-7 - Impostor.md"),
            "---\nid: TASK-7\ntitle: Impostor\n---\n",
        )
        .expect("fixture file");

        let err = resolve_task_file(dir.path(), "TASK-7").expect_err("must refuse");
        assert!(
            err.to_string().contains("2 files"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn subtask_ids_resolve_by_their_full_decimal_form() {
        let dir = project_with_task("task-7 - Fixture.md");
        fs::write(
            dir.path().join("backlog/tasks/task-7.2 - Sub.md"),
            "---\nid: TASK-7.2\ntitle: Sub\n---\n",
        )
        .expect("fixture file");

        let sub = resolve_task_file(dir.path(), "TASK-7.2").expect("resolves");
        assert!(sub.ends_with("task-7.2 - Sub.md"));
        let parent = resolve_task_file(dir.path(), "TASK-7").expect("resolves");
        assert!(parent.ends_with("task-7 - Fixture.md"));
    }

    #[test]
    fn status_validation_matches_the_cli_message_shape() {
        let dir = project_with_task("task-7 - Fixture.md");
        fs::write(
            dir.path().join("backlog/config.yml"),
            "statuses: [\"To Do\", \"Done\"]\n",
        )
        .expect("config fixture");

        assert!(
            validate_status(dir.path(), "to do").is_ok(),
            "case-insensitive"
        );
        let err = validate_status(dir.path(), "Icebox").expect_err("undeclared");
        assert_eq!(
            err.to_string(),
            "Invalid status: Icebox. Valid statuses are: To Do, Done"
        );
    }

    #[test]
    fn a_project_declaring_no_statuses_constrains_nothing() {
        let dir = project_with_task("task-7 - Fixture.md");
        assert!(validate_status(dir.path(), "Anything Goes").is_ok());
    }
}
