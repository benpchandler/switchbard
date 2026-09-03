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

use super::allocate::{claim_task_id, create_task_allocating_id, strip_id_prefix};
use super::ball::Ball;
use super::goals::rename_task_in_goals;
use super::parse::{
    configured_task_prefix, load_backlog_repo, parse_config_statuses, parse_task_file,
    DEFAULT_TASK_PREFIX,
};
use super::ranking::rename_task_in_ranking;
use super::types::{BacklogTask, BacklogTaskPatch, BacklogTaskSource, NewBacklogTask};
use super::write::{
    append_task_acceptance_criteria, append_task_notes, rehome_task_file, replace_task_section,
    revise_task_checklist, set_task_checklist_item, set_task_label, set_task_list_field,
    set_task_priority, set_task_project, set_task_status, set_task_title, swap_task_label,
    ChecklistTextEdit, TaskChecklist, TaskListField, TaskSection, WriteOutcome,
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

/// Reword and/or remove acceptance criteria in one atomic write - the
/// `sb edit --edit-ac N TEXT` / `--remove-ac N` repair path. Every index is
/// the numbering the task showed before this call (edits land before
/// removals), the survivors are renumbered `#1..#n`, and an unknown index
/// fails the whole call with the file untouched. See
/// [`super::write::revise_task_checklist`].
pub fn revise_backlog_acceptance_criteria(
    project_root: &Path,
    task_id: &str,
    edits: &[ChecklistTextEdit],
    removals: &[usize],
) -> Result<String> {
    let path = resolve_task_file(project_root, task_id)?;
    let outcome = revise_task_checklist(&path, TaskChecklist::AcceptanceCriteria, edits, removals)?;
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

/// Hand the ball to `me`, `agent`, or nobody (`None`) — see [`super::ball`].
/// Two surgical label writes at most, each reading the file at write time
/// like [`set_backlog_label`]; a task already in the requested state is a
/// byte no-op that reports `no changes`.
pub fn set_backlog_ball(project_root: &Path, task_id: &str, ball: Option<Ball>) -> Result<String> {
    let path = resolve_task_file(project_root, task_id)?;
    let mut changed = false;
    for holder in [Ball::Me, Ball::Agent] {
        let enabled = ball == Some(holder);
        changed |= set_task_label(&path, Ball::label(holder), enabled)?.changed();
    }
    let outcome = if changed {
        WriteOutcome::Changed
    } else {
        WriteOutcome::Unchanged
    };
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
/// Move a task under a new parent (`Some(id)`) or promote it to top level
/// (`None`), re-minting its id from the reservation allocator and following
/// the old id into every place that names it: other tasks' `dependencies`,
/// `ranking.yml` (`expedite` renamed; the entry in its old sibling scope
/// dropped), and `goals.yml` (`inputs.tasks`). Returns the new full id, or
/// `None` when the task already has that parent and nothing was written.
///
/// One decimal level, the `backlog` CLI's own convention: a task that has
/// sub-issues cannot be moved under a parent (its children would nest two
/// deep), and a sub-issue cannot be the new parent. Only active tasks move;
/// completed and archived ones stay where their status put them.
pub fn move_backlog_task(
    project_root: &Path,
    task_id: &str,
    new_parent: Option<&str>,
) -> Result<Option<String>> {
    let repo = load_backlog_repo(project_root)?;
    let prefix = configured_task_prefix(project_root);
    let task = task_in_repo(&repo.tasks, task_id, &prefix)?;
    if task.source != BacklogTaskSource::Active {
        bail!(
            "{} is {} - only active tasks (backlog/tasks) can be moved",
            task.id,
            task.source.label()
        );
    }
    if repo.tasks.iter().any(|other| {
        other
            .parent
            .as_deref()
            .is_some_and(|p| same_id(p, &task.id, &prefix))
    }) {
        bail!(
            "{} has sub-issues - move or promote them first (sub-issues nest one level)",
            task.id
        );
    }
    let parent = match new_parent {
        Some(wanted) => {
            let parent = task_in_repo(&repo.tasks, wanted, &prefix)?;
            if same_id(&parent.id, &task.id, &prefix) {
                bail!("{} cannot be its own parent", task.id);
            }
            if normalized_id(&parent.id, &prefix).contains('.') {
                bail!(
                    "{} is itself a sub-issue - sub-issues nest one level, pick a top-level parent",
                    parent.id
                );
            }
            Some(parent)
        }
        None => None,
    };
    let current = task.parent.as_deref();
    let unchanged = match (current, parent) {
        (None, None) => true,
        (Some(have), Some(want)) => same_id(have, &want.id, &prefix),
        _ => false,
    };
    if unchanged {
        return Ok(None);
    }

    let parent_bare = parent.map(|p| normalized_id(&p.id, &prefix).to_string());
    let claimed = claim_task_id(project_root, parent_bare.as_deref())?;
    let new_full_id = format!("{prefix}-{}", claimed.id);
    rehome_task_file(&task.path, &prefix, &claimed.id, parent_bare.as_deref())
        .with_context(|| format!("moving {} to {new_full_id}", task.id))?;

    for other in &repo.tasks {
        if !other
            .dependencies
            .iter()
            .any(|dep| same_id(dep, &task.id, &prefix))
        {
            continue;
        }
        let deps: Vec<String> = other
            .dependencies
            .iter()
            .map(|dep| {
                if same_id(dep, &task.id, &prefix) {
                    new_full_id.clone()
                } else {
                    dep.clone()
                }
            })
            .collect();
        let rewritten = set_task_list_field(&other.path, TaskListField::Dependencies, &deps)
            .with_context(|| format!("updating {}'s dependencies", other.id))?;
        if !rewritten.changed() {
            bail!(
                "{}'s dependencies still name {} after rewriting - check the file by hand",
                other.id,
                task.id
            );
        }
    }
    // Unchanged is the common case for both: most tasks are neither ranked
    // nor counted by a goal.
    let _ranking = rename_task_in_ranking(project_root, &task.id, &new_full_id)
        .context("updating backlog/ranking.yml")?;
    let _goals = rename_task_in_goals(project_root, &task.id, &new_full_id)
        .context("updating backlog/goals.yml")?;
    Ok(Some(new_full_id))
}

/// The loaded task whose id matches `wanted` (`TASK-7`, `task-7`, `7`,
/// `7.2` - the same tolerance every `<ID>` argument gets).
fn task_in_repo<'r>(
    tasks: &'r [BacklogTask],
    wanted: &str,
    prefix: &str,
) -> Result<&'r BacklogTask> {
    tasks
        .iter()
        .find(|task| same_id(&task.id, wanted, prefix))
        .with_context(|| format!("no task {wanted} in this repo"))
}

/// Two ids name the same task when their bare parts match, ignoring prefix
/// case (`TASK-7`, `task-7`, and `7` are one task).
fn same_id(a: &str, b: &str, prefix: &str) -> bool {
    normalized_id(a, prefix).eq_ignore_ascii_case(normalized_id(b, prefix))
}

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

    fn labels_of(dir: &tempfile::TempDir) -> Vec<String> {
        let repo = super::super::parse::load_backlog_repo(dir.path()).expect("load");
        repo.tasks[0].labels.clone()
    }

    #[test]
    fn set_backlog_ball_sets_switches_and_drops_the_holder() {
        let dir = project_with_task("task-7 - Fixture.md");
        let root = dir.path();
        assert_eq!(
            set_backlog_ball(root, "TASK-7", Some(Ball::Me)).expect("me"),
            "Edited TASK-7"
        );
        assert_eq!(labels_of(&dir), vec!["ball:me"]);
        assert_eq!(
            set_backlog_ball(root, "TASK-7", Some(Ball::Me)).expect("again"),
            "no changes"
        );
        assert_eq!(
            set_backlog_ball(root, "TASK-7", Some(Ball::Agent)).expect("agent"),
            "Edited TASK-7"
        );
        assert_eq!(labels_of(&dir), vec!["ball:agent"]);
        assert_eq!(
            set_backlog_ball(root, "TASK-7", None).expect("drop"),
            "Edited TASK-7"
        );
        assert!(labels_of(&dir).is_empty());
        assert_eq!(
            set_backlog_ball(root, "TASK-7", None).expect("drop again"),
            "no changes"
        );
    }

    // ---- move_backlog_task ----

    fn task_text(id: &str, extra: &str) -> String {
        format!(
            "---\nid: TASK-{id}\ntitle: Fixture {id}\nstatus: To Do\npriority: medium\n{extra}---\n\n## Description\n\nBody of {id}.\n"
        )
    }

    /// TASK-7 with sub-issue TASK-7.1, TASK-8 depending on TASK-7.1, TASK-7.1
    /// expedited and ranked among 7's sub-issues, and a goal counting it.
    fn move_fixture() -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        let tasks = root.join("backlog/tasks");
        fs::create_dir_all(&tasks).expect("dirs");
        fs::write(root.join("backlog/config.yml"), "statuses: []\n").expect("config");
        fs::write(tasks.join("task-7 - Parent.md"), task_text("7", "")).expect("7");
        fs::write(
            tasks.join("task-7.1 - Child.md"),
            task_text("7.1", "parent_task_id: TASK-7\n"),
        )
        .expect("7.1");
        fs::write(
            tasks.join("task-8 - Other.md"),
            task_text("8", "dependencies:\n  - TASK-7.1\n"),
        )
        .expect("8");
        assert!(super::super::ranking::expedite_task(root, "TASK-7.1")
            .expect("expedite")
            .changed());
        assert!(super::super::ranking::rank_task(
            root,
            "TASK-7.1",
            &super::super::ranking::RankPlacement::Top
        )
        .expect("rank")
        .changed());
        super::super::goals::create_goal(
            root,
            &super::super::goals::NewGoal {
                name: "Ship".to_string(),
                unit: "tasks".to_string(),
                measure: super::super::goals::GoalMeasure::Tasks,
                scope: None,
                week: "2026-08-31".to_string(),
                target: 1,
            },
        )
        .expect("goal");
        super::super::goals::attach_goal_inputs(root, "Ship", &["TASK-7.1".to_string()], &[])
            .expect("attach");
        dir
    }

    fn by_id(root: &Path, id: &str) -> BacklogTask {
        let repo = load_backlog_repo(root).expect("load");
        repo.tasks
            .into_iter()
            .find(|t| t.id == id)
            .unwrap_or_else(|| panic!("no {id}"))
    }

    #[test]
    fn move_reparents_and_every_reference_follows() {
        let dir = move_fixture();
        let root = dir.path();
        let moved = move_backlog_task(root, "7.1", Some("TASK-8")).expect("move");
        assert_eq!(moved.as_deref(), Some("TASK-8.1"));

        assert!(!root.join("backlog/tasks/task-7.1 - Child.md").exists());
        let text =
            fs::read_to_string(root.join("backlog/tasks/task-8.1 - Child.md")).expect("new file");
        assert!(text.contains("id: TASK-8.1\n"), "{text}");
        assert!(text.contains("parent_task_id: TASK-8\n"), "{text}");
        assert!(text.contains("Body of 7.1."), "body kept: {text}");
        assert!(text.contains("updated_date:"), "{text}");
        let moved_task = by_id(root, "TASK-8.1");
        assert_eq!(moved_task.parent.as_deref(), Some("TASK-8"));
        assert_eq!(
            by_id(root, "TASK-8").dependencies,
            vec!["TASK-8.1".to_string()]
        );

        let repo = load_backlog_repo(root).expect("reload");
        assert_eq!(repo.ranking.expedite, vec!["TASK-8.1".to_string()]);
        assert!(
            !repo.ranking.subissues.contains_key("TASK-7"),
            "{:?}",
            repo.ranking
        );
        assert_eq!(repo.goals[0].inputs.tasks, vec!["TASK-8.1".to_string()]);
    }

    #[test]
    fn move_promotes_to_top_level_and_drops_the_parent_key() {
        let dir = move_fixture();
        let root = dir.path();
        let moved = move_backlog_task(root, "TASK-7.1", None).expect("promote");
        assert_eq!(moved.as_deref(), Some("TASK-9"));
        let text =
            fs::read_to_string(root.join("backlog/tasks/task-9 - Child.md")).expect("new file");
        assert!(!text.contains("parent"), "{text}");
        assert_eq!(by_id(root, "TASK-9").parent, None);
        assert_eq!(
            by_id(root, "TASK-8").dependencies,
            vec!["TASK-9".to_string()]
        );
    }

    #[test]
    fn move_refuses_the_shapes_one_decimal_level_forbids() {
        let dir = move_fixture();
        let root = dir.path();
        assert_eq!(
            move_backlog_task(root, "TASK-7.1", Some("7")).expect("same parent"),
            None,
            "unchanged parent is a no-op"
        );
        let own = move_backlog_task(root, "TASK-7.1", Some("TASK-7.1")).expect_err("own parent");
        assert!(
            own.to_string().contains("cannot be its own parent"),
            "{own}"
        );
        let under_child =
            move_backlog_task(root, "TASK-8", Some("TASK-7.1")).expect_err("sub-issue parent");
        assert!(
            under_child.to_string().contains("itself a sub-issue"),
            "{under_child}"
        );
        let has_children =
            move_backlog_task(root, "TASK-7", Some("TASK-8")).expect_err("has children");
        assert!(
            has_children.to_string().contains("has sub-issues"),
            "{has_children}"
        );
        let unknown =
            move_backlog_task(root, "TASK-7.1", Some("TASK-42")).expect_err("unknown parent");
        assert!(unknown.to_string().contains("no task TASK-42"), "{unknown}");
        assert!(
            root.join("backlog/tasks/task-7.1 - Child.md").exists(),
            "refusals never move the file"
        );
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
