//! End-to-end proof that every `switchbard_core::backlog` mutation function
//! persists through the native write layer against a real on-disk project —
//! the successor to `backlog_mutations.rs`, which proved the same
//! surface through the external `backlog` CLI before the format fork's
//! TASK-65 swap. No CLI, no git required for the fixtures: a Backlog project
//! is a directory shape, and the write layer owns it end to end.
//!
//! Context (QA parity audit, 2026-08-05, carried forward): the GUI's
//! detail-pane editors each queue a `Pending` action that a background
//! thread applies by calling exactly one of the functions covered here (see
//! `switchbard-gui/src/app.rs`'s `spawn_backlog_*` methods). Proving the
//! click *queues the right pending value* is `switchbard-gui`'s job
//! (`tests/backlog_controls.rs`); proving that pending value, once applied,
//! *actually changes the task on disk* is this file's job.

use std::fs;
use std::path::Path;

use switchbard_core::{
    append_backlog_notes, archive_backlog_task, build_refine_patch, claim_task_for_dispatch,
    complete_backlog_task, create_backlog_task, edit_backlog_task, load_backlog_project,
    set_backlog_acceptance_checked, set_backlog_dod_checked, set_backlog_label, swap_backlog_label,
    BacklogTaskPatch, BacklogTaskSource, NewBacklogTask, RefineSuggestion, REFINED_MARKER,
};
use tempfile::TempDir;

/// A throwaway Backlog project: the directory shape plus a `config.yml`
/// declaring the standard status vocabulary (which `edit_backlog_task`'s
/// status validation reads).
fn fixture_project() -> TempDir {
    let dir = TempDir::new().expect("create temp dir");
    fs::create_dir_all(dir.path().join("backlog/tasks")).expect("project layout");
    fs::write(
        dir.path().join("backlog/config.yml"),
        "project_name: \"fixture\"\n\
         statuses: [\"Icebox\", \"To Do\", \"In Progress\", \"In Review\", \"Done\"]\n",
    )
    .expect("config fixture");
    dir
}

fn create_fixture_task(root: &Path) -> String {
    let task = NewBacklogTask {
        title: "Fixture task".to_string(),
        description: "Initial description".to_string(),
        status: "To Do".to_string(),
        priority: "medium".to_string(),
        acceptance_criteria: vec!["First criterion".to_string()],
        parent: None,
        labels: vec![],
        assignees: vec![],
        milestone: None,
        dependencies: vec![],
    };
    let output = create_backlog_task(root, &task).expect("create_backlog_task should succeed");
    assert_eq!(
        output, "TASK-1",
        "create now returns exactly the new task's id"
    );
    output
}

fn reload(root: &Path) -> switchbard_core::BacklogProject {
    load_backlog_project(root).expect("reparsing the fixture project should succeed")
}

fn find<'p>(
    project: &'p switchbard_core::BacklogProject,
    id: &str,
) -> &'p switchbard_core::BacklogTask {
    project
        .tasks
        .iter()
        .find(|t| t.id == id)
        .unwrap_or_else(|| panic!("{id} should be present"))
}

#[test]
fn edit_backlog_task_persists_every_field_the_detail_pane_editor_exposes() {
    let fixture = fixture_project();
    let root = fixture.path();
    let task_id = create_fixture_task(root);

    let patch = BacklogTaskPatch {
        title: Some("Renamed fixture".to_string()),
        description: Some("New description.\n\nSecond paragraph.".to_string()),
        status: Some("In Progress".to_string()),
        priority: Some("high".to_string()),
        labels: Some(vec!["alpha".to_string(), "beta".to_string()]),
        assignees: Some(vec!["ben".to_string()]),
        dependencies: Some(vec!["task-9".to_string()]),
        references: Some(vec!["https://example.com/spec".to_string()]),
        implementation_plan: Some("1. Do it\n2. Prove it".to_string()),
        milestone: Some("m-1".to_string()),
        ..Default::default()
    };
    edit_backlog_task(root, &task_id, &patch).expect("edit should succeed");

    let project = reload(root);
    let task = find(&project, &task_id);
    assert_eq!(task.title, "Renamed fixture");
    assert_eq!(task.description, "New description.\n\nSecond paragraph.");
    assert_eq!(task.status, "In Progress");
    assert_eq!(task.priority, "high");
    assert_eq!(task.labels, vec!["alpha", "beta"]);
    assert_eq!(task.assignees, vec!["ben"]);
    assert_eq!(task.dependencies, vec!["task-9"]);
    assert_eq!(task.references, vec!["https://example.com/spec"]);
    assert_eq!(task.implementation_plan, "1. Do it\n2. Prove it");
    assert_eq!(task.milestone.as_deref(), Some("m-1"));
    assert_eq!(
        task.acceptance_criteria.len(),
        1,
        "the patch never touched criteria"
    );

    edit_backlog_task(
        root,
        &task_id,
        &BacklogTaskPatch {
            clear_milestone: true,
            ..Default::default()
        },
    )
    .expect("clearing the milestone should succeed");
    let project = reload(root);
    assert_eq!(find(&project, &task_id).milestone, None);
}

#[test]
fn an_undeclared_status_is_rejected_with_the_cli_message_shape() {
    let fixture = fixture_project();
    let root = fixture.path();
    let task_id = create_fixture_task(root);

    let err = edit_backlog_task(
        root,
        &task_id,
        &BacklogTaskPatch {
            status: Some("Blocked".to_string()),
            ..Default::default()
        },
    )
    .expect_err("an undeclared status must be refused");

    assert!(
        err.to_string()
            .starts_with("Invalid status: Blocked. Valid statuses are:"),
        "the status-vocabulary offer flow keys off this shape: {err}"
    );
    let project = reload(root);
    assert_eq!(
        find(&project, &task_id).status,
        "To Do",
        "a refused edit must change nothing"
    );
}

#[test]
fn acceptance_and_definition_of_done_checkboxes_persist() {
    let fixture = fixture_project();
    let root = fixture.path();
    let task_id = create_fixture_task(root);
    // `create_backlog_task` has no DoD parameter (same as the CLI's create);
    // give the fixture a DoD section the way the format defines one.
    let project = reload(root);
    let path = find(&project, &task_id).path.clone();
    let mut text = fs::read_to_string(&path).expect("fixture reads");
    text.push_str(
        "\n## Definition of Done\n<!-- DOD:BEGIN -->\n- [ ] #1 Ship it\n<!-- DOD:END -->\n",
    );
    fs::write(&path, text).expect("fixture writes");

    set_backlog_acceptance_checked(root, &task_id, 1, true).expect("check AC");
    set_backlog_dod_checked(root, &task_id, 1, true).expect("check DoD");

    let project = reload(root);
    let task = find(&project, &task_id);
    assert!(task.acceptance_criteria[0].checked);
    assert!(task.definition_of_done[0].checked);

    set_backlog_acceptance_checked(root, &task_id, 1, false).expect("uncheck AC");
    let project = reload(root);
    assert!(!find(&project, &task_id).acceptance_criteria[0].checked);
}

#[test]
fn appended_acceptance_criteria_extend_the_list_without_disturbing_existing_ones() {
    let fixture = fixture_project();
    let root = fixture.path();
    let task_id = create_fixture_task(root);
    set_backlog_acceptance_checked(root, &task_id, 1, true).expect("check the first");

    edit_backlog_task(
        root,
        &task_id,
        &BacklogTaskPatch {
            append_acceptance_criteria: vec!["Second criterion".to_string()],
            ..Default::default()
        },
    )
    .expect("append should succeed");

    let project = reload(root);
    let task = find(&project, &task_id);
    assert_eq!(task.acceptance_criteria.len(), 2);
    assert!(
        task.acceptance_criteria[0].checked,
        "appending must never disturb an existing criterion's checked state"
    );
    assert_eq!(task.acceptance_criteria[1].text, "Second criterion");
    assert_eq!(task.acceptance_criteria[1].index, 2);
}

/// The refine idempotence contract, end to end through the native writer:
/// refine normalizes blank runs *before* writing (the CLI used to normalize
/// on write), so a second identical refine finds its own previous output
/// and appends nothing.
#[test]
fn a_second_refine_appends_nothing_after_the_first_normalized_its_blank_runs() {
    let fixture = fixture_project();
    let root = fixture.path();
    let task_id = create_fixture_task(root);

    let suggestion = RefineSuggestion {
        description: "First refined paragraph.\n\n\n\nSecond refined paragraph.".to_string(),
        acceptance_criteria: vec![],
        implementation_plan: String::new(),
    };

    let before = reload(root);
    let first =
        build_refine_patch(find(&before, &task_id), &suggestion, true).expect("first merge");
    assert!(first.description_extended, "the first refine should extend");
    edit_backlog_task(root, &task_id, &first.patch).expect("first refine write");

    let after = reload(root);
    let task = find(&after, &task_id);
    assert!(
        task.description.starts_with("Initial description"),
        "the original must still lead the description: {:?}",
        task.description
    );
    assert!(
        !task.description.contains("\n\n\n"),
        "refine normalizes before writing, so no blank run may reach disk"
    );

    let second = build_refine_patch(task, &suggestion, true).expect("second merge");
    assert!(
        second.patch.description.is_none(),
        "re-refining identical prose must be a no-op, got {:?}",
        second.patch.description
    );
    assert_eq!(
        task.description.matches(REFINED_MARKER).count(),
        1,
        "exactly one refined block should exist on disk"
    );
}

/// The write-path round-trip guard against a native-written file carrying a
/// fenced `## ` heading — the shape that used to be silently truncated on
/// the next save (TASK-44).
#[test]
fn a_native_written_task_round_trips_through_the_parser_even_with_a_fenced_heading() {
    let fixture = fixture_project();
    let root = fixture.path();
    let task_id = create_fixture_task(root);
    let description = "Intro.\n\n```markdown\n## Not a section\n```\n\nOutro.";

    edit_backlog_task(
        root,
        &task_id,
        &BacklogTaskPatch {
            description: Some(description.to_string()),
            ..Default::default()
        },
    )
    .expect("edit should succeed");

    let project = reload(root);
    let task = find(&project, &task_id);
    assert_eq!(task.description, description);
    assert!(
        switchbard_core::task_file_round_trips(&task.path),
        "the write layer must only ever produce files it can safely rewrite"
    );
    assert_eq!(task.acceptance_criteria.len(), 1, "later sections intact");
}

/// TASK-45, the Save path end to end: a human-written section the Backlog
/// format has no field for (`## Resolution` on 51 of 345 real task files
/// measured during TASK-44) must neither block a description save nor be
/// deleted by it. This is the exact route the detail rail's Save takes —
/// `edit_backlog_task` with a description patch.
#[test]
fn saving_a_description_preserves_custom_sections_the_format_does_not_model() {
    let fixture = fixture_project();
    let root = fixture.path();
    let task_id = create_fixture_task(root);
    let path = find(&reload(root), &task_id).path.clone();
    let custom_block = "## Resolution\n\nRoot cause: the cache.\n";
    let mut text = fs::read_to_string(&path).expect("read fixture task");
    text.push_str(&format!("\n{custom_block}"));
    fs::write(&path, &text).expect("hand-append a custom section");

    edit_backlog_task(
        root,
        &task_id,
        &BacklogTaskPatch {
            description: Some("Updated description".to_string()),
            ..Default::default()
        },
    )
    .expect("a custom section must not block the save");

    let saved = fs::read_to_string(&path).expect("reread task file");
    assert!(
        saved.contains(custom_block),
        "the custom section must survive a description save verbatim: {saved}"
    );
    let project = reload(root);
    assert_eq!(find(&project, &task_id).description, "Updated description");
}

#[test]
fn append_notes_persists_and_accumulates() {
    let fixture = fixture_project();
    let root = fixture.path();
    let task_id = create_fixture_task(root);

    append_backlog_notes(root, &task_id, "First note.").expect("first note");
    append_backlog_notes(root, &task_id, "Second note.").expect("second note");

    let project = reload(root);
    let notes = &find(&project, &task_id).implementation_notes;
    assert!(notes.contains("First note."), "{notes:?}");
    assert!(notes.contains("Second note."), "{notes:?}");
}

#[test]
fn append_notes_rejects_an_empty_note_without_touching_the_file() {
    let fixture = fixture_project();
    let root = fixture.path();
    let task_id = create_fixture_task(root);

    let err = append_backlog_notes(root, &task_id, "   ").expect_err("empty note must fail");
    assert!(err.to_string().contains("empty"), "unexpected error: {err}");
}

#[test]
fn archive_moves_the_task_out_of_the_active_set() {
    let fixture = fixture_project();
    let root = fixture.path();
    let task_id = create_fixture_task(root);

    let output = archive_backlog_task(root, &task_id).expect("archive should succeed");
    assert_eq!(output, "Archived task TASK-1");

    let project = reload(root);
    let task = find(&project, &task_id);
    assert_eq!(task.source, BacklogTaskSource::Archived);
    assert!(!task.editable(), "archived tasks are read-only in the GUI");
}

#[test]
fn archiving_a_done_task_is_rejected_and_completing_requires_done() {
    let fixture = fixture_project();
    let root = fixture.path();
    let task_id = create_fixture_task(root);

    let err = complete_backlog_task(root, &task_id).expect_err("To Do cannot complete");
    assert!(err.to_string().contains("Done"), "unexpected error: {err}");

    edit_backlog_task(
        root,
        &task_id,
        &BacklogTaskPatch {
            status: Some("Done".to_string()),
            ..Default::default()
        },
    )
    .expect("move to Done");

    let err = archive_backlog_task(root, &task_id).expect_err("Done cannot archive");
    assert!(
        err.to_string().contains("completed"),
        "the refusal must point at complete: {err}"
    );

    let output = complete_backlog_task(root, &task_id).expect("complete should succeed");
    assert_eq!(output, "Completed task TASK-1");
    let project = reload(root);
    assert_eq!(
        find(&project, &task_id).source,
        BacklogTaskSource::Completed
    );
}

#[test]
fn set_backlog_label_adds_and_removes_a_single_label_without_touching_others() {
    let fixture = fixture_project();
    let root = fixture.path();
    let task_id = create_fixture_task(root);
    set_backlog_label(root, &task_id, "keep-me", true).expect("add keep-me");

    set_backlog_label(root, &task_id, "dispatch", true).expect("add dispatch");
    let project = reload(root);
    assert_eq!(find(&project, &task_id).labels, vec!["keep-me", "dispatch"]);

    set_backlog_label(root, &task_id, "dispatch", false).expect("remove dispatch");
    let project = reload(root);
    assert_eq!(find(&project, &task_id).labels, vec!["keep-me"]);
}

#[test]
fn swap_backlog_label_replaces_atomically_and_is_strict_about_the_source() {
    let fixture = fixture_project();
    let root = fixture.path();
    let task_id = create_fixture_task(root);
    set_backlog_label(root, &task_id, "dispatch", true).expect("flag");

    swap_backlog_label(root, &task_id, "dispatch", "dispatching").expect("swap");
    let project = reload(root);
    assert_eq!(find(&project, &task_id).labels, vec!["dispatching"]);

    // Strict claim semantics (deliberately stronger than the CLI, which
    // added the target label anyway): a second claimant must lose, not
    // half-win.
    let err = swap_backlog_label(root, &task_id, "dispatch", "dispatching")
        .expect_err("the token is gone; the swap must fail");
    assert!(
        err.to_string().contains("dispatch"),
        "unexpected error: {err}"
    );
    let project = reload(root);
    assert_eq!(
        find(&project, &task_id).labels,
        vec!["dispatching"],
        "a failed claim must change nothing"
    );
}

#[test]
fn claiming_a_task_clears_the_previous_attempts_terminal_labels() {
    let fixture = fixture_project();
    let root = fixture.path();
    let task_id = create_fixture_task(root);
    set_backlog_label(root, &task_id, "dispatch-failed", true).expect("fixture label");
    set_backlog_label(root, &task_id, "keep-me", true).expect("fixture label");
    set_backlog_label(root, &task_id, "dispatch", true).expect("fixture label");

    claim_task_for_dispatch(root, &task_id).expect("claim should succeed");

    let project = reload(root);
    let task = find(&project, &task_id);
    assert!(task.labels.contains(&"dispatching".to_string()));
    assert!(
        !task.labels.contains(&"dispatch-failed".to_string()),
        "the previous run's verdict must not survive the new claim: {:?}",
        task.labels
    );
    assert!(!task.labels.contains(&"dispatch".to_string()));
    assert!(
        task.labels.contains(&"keep-me".to_string()),
        "unrelated labels are none of the claim's business: {:?}",
        task.labels
    );
}

#[test]
fn claiming_a_task_clears_a_previous_successful_runs_label() {
    let fixture = fixture_project();
    let root = fixture.path();
    let task_id = create_fixture_task(root);
    set_backlog_label(root, &task_id, "dispatched", true).expect("fixture label");
    set_backlog_label(root, &task_id, "dispatch", true).expect("fixture label");

    claim_task_for_dispatch(root, &task_id).expect("claim should succeed");

    let project = reload(root);
    let task = find(&project, &task_id);
    assert!(task.labels.contains(&"dispatching".to_string()));
    assert!(!task.labels.contains(&"dispatched".to_string()));
}

#[test]
fn claiming_a_never_dispatched_task_is_a_plain_swap() {
    let fixture = fixture_project();
    let root = fixture.path();
    let task_id = create_fixture_task(root);
    set_backlog_label(root, &task_id, "dispatch", true).expect("fixture label");

    claim_task_for_dispatch(root, &task_id).expect("claim should succeed");

    let project = reload(root);
    assert_eq!(find(&project, &task_id).labels, vec!["dispatching"]);
}

#[test]
fn create_backlog_task_wires_labels_assignee_milestone_and_dependencies() {
    let fixture = fixture_project();
    let root = fixture.path();
    let dependency_id = create_fixture_task(root);

    let output = create_backlog_task(
        root,
        &NewBacklogTask {
            title: "Fully specified task".to_string(),
            description: String::new(),
            status: "To Do".to_string(),
            priority: "medium".to_string(),
            acceptance_criteria: vec![],
            parent: None,
            labels: vec!["frontend".to_string(), "urgent".to_string()],
            assignees: vec!["ben".to_string()],
            milestone: Some("v1".to_string()),
            dependencies: vec![dependency_id.clone()],
        },
    )
    .expect("create with labels/assignee/milestone/dependencies should succeed");
    assert_eq!(output, "TASK-2");

    let project = reload(root);
    let task = find(&project, "TASK-2");
    assert_eq!(task.labels, vec!["frontend", "urgent"]);
    assert_eq!(task.assignees, vec!["ben"]);
    assert_eq!(task.milestone.as_deref(), Some("v1"));
    assert_eq!(task.dependencies, vec![dependency_id]);
}

/// Subtasks keep the CLI's decimal-child convention end to end: the child of
/// `TASK-1` is `TASK-1.1`, written with `parent_task_id:` in frontmatter
/// (the key the 2026-08-05 QA audit proved the format actually uses).
#[test]
fn subtask_ids_are_decimal_children_of_the_parent_id() {
    let fixture = fixture_project();
    let root = fixture.path();
    let parent_id = create_fixture_task(root);

    for title in ["First subtask", "Second subtask"] {
        create_backlog_task(
            root,
            &NewBacklogTask {
                title: title.to_string(),
                description: String::new(),
                status: "To Do".to_string(),
                priority: "medium".to_string(),
                acceptance_criteria: vec![],
                parent: Some(parent_id.clone()),
                labels: vec![],
                assignees: vec![],
                milestone: None,
                dependencies: vec![],
            },
        )
        .expect("subtask create should succeed");
    }

    let project = reload(root);
    let mut child_ids: Vec<&str> = project
        .tasks
        .iter()
        .filter(|t| t.parent.as_deref() == Some(parent_id.as_str()))
        .map(|t| t.id.as_str())
        .collect();
    child_ids.sort();
    assert_eq!(
        child_ids,
        vec![
            format!("{parent_id}.1").as_str(),
            format!("{parent_id}.2").as_str(),
        ],
        "subtasks number as decimal children of the parent id"
    );
}

#[test]
fn two_independent_fixture_projects_can_both_mint_task_1() {
    let repo_a = fixture_project();
    let repo_b = fixture_project();
    let id_a = create_fixture_task(repo_a.path());
    let id_b = create_fixture_task(repo_b.path());
    assert_eq!(id_a, "TASK-1");
    assert_eq!(id_b, "TASK-1");

    edit_backlog_task(
        repo_a.path(),
        &id_a,
        &BacklogTaskPatch {
            title: Some("Alpha's TASK-1".to_string()),
            ..Default::default()
        },
    )
    .expect("edit repo A");

    let project_b = reload(repo_b.path());
    assert_eq!(
        find(&project_b, "TASK-1").title,
        "Fixture task",
        "editing repo A's TASK-1 must not affect repo B's own TASK-1"
    );
}

/// The reproduction (LED-* on staging): a project whose `backlog/config.yml`
/// declares `task_prefix: "LED"` (budget's own config) must mint an id and
/// filename in that family, continuing past an existing `led-10` file — not
/// mint `TASK-1`, a file the external `backlog` CLI in that project would
/// never recognize as one of its own tasks.
#[test]
fn create_backlog_task_mints_the_projects_configured_prefix() {
    let fixture = fixture_project();
    let root = fixture.path();
    fs::write(
        root.join("backlog/config.yml"),
        "project_name: \"fixture\"\n\
         statuses: [\"Icebox\", \"To Do\", \"In Progress\", \"In Review\", \"Done\"]\n\
         task_prefix: \"LED\"\n",
    )
    .expect("rewrite config with task_prefix");
    fs::write(
        root.join("backlog/tasks/led-10 - Existing.md"),
        "---\nid: LED-10\ntitle: Existing\nstatus: To Do\npriority: medium\n---\n",
    )
    .expect("seed an existing led-10 file");

    let output = create_backlog_task(
        root,
        &NewBacklogTask {
            title: "Fix the prefix bug".to_string(),
            description: String::new(),
            status: "To Do".to_string(),
            priority: "medium".to_string(),
            acceptance_criteria: vec![],
            parent: None,
            labels: vec![],
            assignees: vec![],
            milestone: None,
            dependencies: vec![],
        },
    )
    .expect("create_backlog_task should succeed");

    assert_eq!(
        output, "LED-11",
        "must continue LED numbering past the existing led-10 file, not mint TASK-1"
    );
    assert!(
        root.join("backlog/tasks/led-11 - Fix-the-prefix-bug.md")
            .is_file(),
        "the created file must use the CLI's led- filename convention"
    );

    let project = reload(root);
    assert_eq!(find(&project, "LED-11").title, "Fix the prefix bug");
}

#[test]
fn editing_a_missing_task_names_the_id_and_a_duplicate_is_refused() {
    let fixture = fixture_project();
    let root = fixture.path();
    create_fixture_task(root);

    let err = edit_backlog_task(
        root,
        "TASK-99",
        &BacklogTaskPatch {
            priority: Some("high".to_string()),
            ..Default::default()
        },
    )
    .expect_err("unknown id must fail");
    assert!(
        err.to_string().contains("TASK-99"),
        "unexpected error: {err}"
    );

    fs::write(
        root.join("backlog/tasks/task-1 - Impostor.md"),
        "---\nid: TASK-1\ntitle: Impostor\n---\n",
    )
    .expect("fixture duplicate");
    let err = append_backlog_notes(root, "TASK-1", "note").expect_err("duplicate must be refused");
    assert!(
        err.to_string().contains("resolve the duplicate"),
        "unexpected error: {err}"
    );
}

#[test]
fn an_unreadable_task_file_becomes_a_warning_not_a_load_failure() {
    let fixture = fixture_project();
    let root = fixture.path();
    create_fixture_task(root);
    let malformed = root.join("backlog/tasks/task-99 - Malformed.md");
    fs::write(&malformed, [0xFF, 0xFE, 0x00, 0xFF]).expect("fixture writes");

    let project = reload(root);
    assert!(
        project.tasks.iter().any(|t| t.id == "TASK-1"),
        "the well-formed task should still load"
    );
    assert!(
        !project.warnings.is_empty(),
        "an unreadable task file should surface as a warning, not abort the whole project load"
    );
}
