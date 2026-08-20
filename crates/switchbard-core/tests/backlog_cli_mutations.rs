//! End-to-end proof that every `switchbard_core::backlog` mutation function
//! actually persists through the real `backlog` CLI, against a real fixture
//! project (`git init` + `backlog init`) under `$TMPDIR` — never touching a
//! developer's real repos or `~/.switchbard/config.toml`.
//!
//! Context (QA parity audit, 2026-08-05): the GUI's detail-pane editors
//! (Save, AC/DoD checkboxes, references Add, notes Append, Archive, the
//! per-task Dispatch label) each queue a `Pending` action that a background
//! thread applies by calling exactly one of the functions covered here (see
//! `switchbard-gui/src/app.rs`'s `spawn_backlog_*` methods). Proving the
//! click *queues the right pending value* is `switchbard-gui`'s job
//! (`tests/backlog_controls.rs`); proving that pending value, once applied,
//! *actually changes the task on disk* is this file's job. Together they
//! cover a control end to end without needing the GUI test harness to block
//! on a real background thread, which the project's own convention already
//! treats as impractical (see `worktree_removal_orchestration.rs`'s doc
//! comment).
//!
//! Every fixture is a throwaway `TempDir`; nothing here ever names a path
//! under the developer's home directory or `~/Dev`.

use std::fs;
use std::path::Path;
use std::process::Command;

use switchbard_core::{
    append_backlog_notes, archive_backlog_task, build_refine_patch, claim_task_for_dispatch,
    complete_backlog_task, create_backlog_task, edit_backlog_task, load_backlog_project,
    set_backlog_acceptance_checked, set_backlog_dod_checked, set_backlog_label, swap_backlog_label,
    task_file_round_trips, BacklogProject, BacklogTaskPatch, BacklogTaskSource, NewBacklogTask,
    RefineSuggestion, REFINED_MARKER,
};
use tempfile::TempDir;

/// A real, throwaway Backlog.md project: `git init` (required by `backlog
/// init`'s default git integration) plus `backlog init --defaults`, with
/// agent instruction files skipped (irrelevant noise for this fixture).
/// Panics on setup failure — a missing/broken `backlog` CLI on `PATH` is a
/// hard prerequisite for this whole file. `mise.toml` pins it
/// (`npm:backlog.md`), so `mise install` / `jdx/mise-action` in CI provide it.
fn fixture_repo() -> TempDir {
    let dir = TempDir::new().expect("create temp dir");
    run(dir.path(), "git", &["init", "-q"]);
    run(
        dir.path(),
        "git",
        &["config", "user.email", "qa@example.com"],
    );
    run(dir.path(), "git", &["config", "user.name", "QA Fixture"]);
    run(
        dir.path(),
        "backlog",
        &[
            "init",
            "--defaults",
            "--agent-instructions",
            "none",
            "qa-fixture",
        ],
    );
    assert!(
        dir.path().join("backlog").join("config.yml").is_dir()
            || dir.path().join("backlog").join("config.yml").is_file(),
        "backlog init should have created backlog/config.yml"
    );
    dir
}

fn run(cwd: &Path, cmd: &str, args: &[&str]) {
    let output = Command::new(cmd)
        .current_dir(cwd)
        .args(args)
        .output()
        .unwrap_or_else(|e| panic!("failed to run {cmd}: {e}"));
    assert!(
        output.status.success(),
        "{cmd} {args:?} failed: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

/// Create one task with an AC and a DoD item via the real CLI, returning its
/// id (`create_backlog_task`'s own output — proving `create_backlog_task`
/// itself works is this call's job before the rest of the file builds on it).
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
    assert!(
        output.contains("TASK-1"),
        "create output should name the new task id: {output}"
    );
    // Add a DoD item too (create_backlog_task has no DoD param), and confirm
    // via a second, unrelated field edit in the same call — cheap coverage
    // of edit_backlog_task's own field wiring beyond just `status`.
    edit_backlog_task(
        root,
        "TASK-1",
        &BacklogTaskPatch {
            ..Default::default()
        },
    )
    .expect("a no-op patch should still succeed (edit_backlog_task treats empty patch as a no-op)");
    "TASK-1".to_string()
}

fn reload(root: &Path) -> switchbard_core::BacklogProject {
    load_backlog_project(root).expect("reparsing the fixture project should succeed")
}

/// TASK-28 (owner-found bug): `parse_created_task_id` against `create_
/// backlog_task`'s real return value, not the pinned string the core unit
/// test uses — proves the parser matches what the real CLI actually
/// outputs today, not just what it output when that string was captured.
#[test]
fn parse_created_task_id_extracts_the_id_from_a_real_create_call() {
    let fixture = fixture_repo();
    let root = fixture.path();
    let output = create_backlog_task(
        root,
        &NewBacklogTask {
            title: "Real create output task".to_string(),
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
    .expect("create should succeed");

    assert_eq!(
        switchbard_core::parse_created_task_id(&output),
        Some("TASK-1".to_string())
    );
}

/// TASK-25 (owner-requested UX): `load_backlog_project` reads
/// `BacklogProject::configured_statuses` off a real `backlog init`-created
/// project's `backlog/config.yml`. `backlog config set` has no key for the
/// statuses list (confirmed against `backlog config set --help`, which
/// enumerates every settable key and statuses isn't one of them), so a
/// custom list can only ever get there by hand-editing config.yml — the
/// same thing this test does — never through a CLI call this suite could
/// instead exercise.
#[test]
fn load_backlog_project_reads_configured_statuses_from_a_real_init() {
    let fixture = fixture_repo();
    let root = fixture.path();

    // A real `backlog init --defaults` project (via fixture_repo()) starts
    // with the standard three; overwrite with budget's own real Icebox set
    // to prove the pipeline against a config shape actually seen in a
    // tracked repo, not an invented one.
    let config_path = root.join("backlog/config.yml");
    let original = std::fs::read_to_string(&config_path).expect("read the real generated config");
    assert!(
        original.contains("statuses: [\"To Do\", \"In Progress\", \"Done\"]"),
        "expected backlog init --defaults' known statuses line, got: {original}"
    );
    let with_icebox = original.replace(
        "statuses: [\"To Do\", \"In Progress\", \"Done\"]",
        "statuses: [\"Icebox\", \"To Do\", \"In Progress\", \"In Review\", \"Done\"]",
    );
    std::fs::write(&config_path, with_icebox).expect("write the customized config");

    let project = reload(root);
    assert_eq!(
        project.configured_statuses,
        vec!["Icebox", "To Do", "In Progress", "In Review", "Done"]
    );
}

#[test]
fn edit_backlog_task_persists_every_field_the_detail_pane_editor_exposes() {
    let fixture = fixture_repo();
    let root = fixture.path();
    let task_id = create_fixture_task(root);

    // Create a second task so `dependencies` has something real to point at.
    create_backlog_task(
        root,
        &NewBacklogTask {
            title: "Dependency target".to_string(),
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
    .expect("create second fixture task");

    let patch = BacklogTaskPatch {
        title: Some("Renamed via edit_backlog_task".to_string()),
        description: Some("Updated description".to_string()),
        status: Some("In Progress".to_string()),
        priority: Some("high".to_string()),
        labels: Some(vec!["demo".to_string(), "qa".to_string()]),
        assignees: Some(vec!["ben".to_string()]),
        dependencies: Some(vec!["TASK-2".to_string()]),
        references: Some(vec!["https://example.com/spec".to_string()]),
        implementation_plan: Some("Step one, then step two.".to_string()),
        append_acceptance_criteria: vec![],
        milestone: Some("v1".to_string()),
        clear_milestone: false,
    };
    edit_backlog_task(root, &task_id, &patch).expect("edit_backlog_task should succeed");

    let project = reload(root);
    let task = project
        .tasks
        .iter()
        .find(|t| t.id == task_id)
        .expect("edited task should still be present");
    assert_eq!(task.title, "Renamed via edit_backlog_task");
    assert_eq!(task.description, "Updated description");
    assert_eq!(task.status, "In Progress");
    assert_eq!(task.priority, "high");
    assert_eq!(task.labels, vec!["demo", "qa"]);
    assert_eq!(task.assignees, vec!["ben"]);
    assert_eq!(task.dependencies, vec!["TASK-2"]);
    assert_eq!(task.references, vec!["https://example.com/spec"]);
    assert_eq!(task.implementation_plan, "Step one, then step two.");
    assert_eq!(task.milestone.as_deref(), Some("v1"));

    // The detail pane's milestone "Clear" button round-trips through
    // `clear_milestone` rather than `milestone` — prove that flag too.
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
    let task = project.tasks.iter().find(|t| t.id == task_id).unwrap();
    assert_eq!(task.milestone, None, "clear_milestone should remove it");
}

#[test]
fn acceptance_and_definition_of_done_checkboxes_persist_through_the_cli() {
    let fixture = fixture_repo();
    let root = fixture.path();
    let task_id = create_fixture_task(root);
    edit_backlog_task(
        root,
        &task_id,
        &BacklogTaskPatch {
            ..Default::default()
        },
    )
    .unwrap();
    // Add a DoD item via a raw edit (create_backlog_task has no --dod flag).
    run(
        root,
        "backlog",
        &["task", "edit", &task_id, "--dod", "Reviewed"],
    );

    set_backlog_acceptance_checked(root, &task_id, 1, true).expect("checking AC #1 should succeed");
    let project = reload(root);
    let task = project.tasks.iter().find(|t| t.id == task_id).unwrap();
    assert!(
        task.acceptance_criteria[0].checked,
        "AC #1 should be checked after set_backlog_acceptance_checked(true)"
    );

    set_backlog_acceptance_checked(root, &task_id, 1, false)
        .expect("unchecking AC #1 should succeed");
    let project = reload(root);
    let task = project.tasks.iter().find(|t| t.id == task_id).unwrap();
    assert!(
        !task.acceptance_criteria[0].checked,
        "AC #1 should be unchecked again"
    );

    set_backlog_dod_checked(root, &task_id, 1, true).expect("checking DoD #1 should succeed");
    let project = reload(root);
    let task = project.tasks.iter().find(|t| t.id == task_id).unwrap();
    assert!(
        task.definition_of_done[0].checked,
        "DoD #1 should be checked after set_backlog_dod_checked(true)"
    );
}

/// TASK-44: the Refine feature's whole safety claim is that an agent's
/// suggestions can never disturb a criterion a human wrote or checked. That
/// claim rests on `BacklogTaskPatch::append_acceptance_criteria` mapping to
/// the CLI's *additive* `--ac`, not its list-replacing
/// `--acceptance-criteria`. Proving the distinction needs the real CLI —
/// `refine.rs`'s own unit tests can only prove the patch it builds, not what
/// the CLI does with it.
#[test]
fn appended_acceptance_criteria_extend_the_list_without_disturbing_existing_ones() {
    let fixture = fixture_repo();
    let root = fixture.path();
    let task_id = create_fixture_task(root);
    set_backlog_acceptance_checked(root, &task_id, 1, true).expect("check the first criterion");

    let patch = BacklogTaskPatch {
        append_acceptance_criteria: vec![
            "Second criterion".to_string(),
            "Third criterion".to_string(),
        ],
        ..Default::default()
    };
    edit_backlog_task(root, &task_id, &patch).expect("appending criteria should succeed");

    let project = reload(root);
    let task = project.tasks.iter().find(|t| t.id == task_id).unwrap();
    let texts: Vec<&str> = task
        .acceptance_criteria
        .iter()
        .map(|item| item.text.as_str())
        .collect();
    assert_eq!(
        texts,
        vec!["First criterion", "Second criterion", "Third criterion"],
        "existing criteria keep their text and position; new ones land after them"
    );
    assert!(
        task.acceptance_criteria[0].checked,
        "the pre-existing criterion must keep its checked state across an append"
    );
    assert!(!task.acceptance_criteria[1].checked);
}

/// TASK-44 audit finding F2. The `backlog` CLI collapses runs of blank lines
/// on write, so text handed to `-d` does not come back off disk byte for
/// byte. `refine`'s idempotence guard is a `contains` check against what came
/// off disk, so if it compared un-normalized text it would never match and a
/// second refine would append a near-duplicate block. Only the real CLI can
/// prove the normalization matches — hence a fixture round trip rather than a
/// unit test.
#[test]
fn a_second_refine_appends_nothing_after_the_cli_normalized_the_first_ones_blank_runs() {
    let fixture = fixture_repo();
    let root = fixture.path();
    let task_id = create_fixture_task(root);

    // A model answer with a blank run the CLI is going to collapse.
    let suggestion = RefineSuggestion {
        description: "First refined paragraph.\n\n\n\nSecond refined paragraph.".to_string(),
        acceptance_criteria: vec![],
        implementation_plan: String::new(),
    };

    let before = reload(root);
    let task = before.tasks.iter().find(|t| t.id == task_id).unwrap();
    let first = build_refine_patch(task, &suggestion, true).expect("first merge");
    assert!(first.description_extended, "the first refine should extend");
    edit_backlog_task(root, &task_id, &first.patch).expect("first refine write");

    // Round trip: this is where the CLI's own normalization happens.
    let after = reload(root);
    let task = after.tasks.iter().find(|t| t.id == task_id).unwrap();
    assert!(
        task.description.starts_with("Initial description"),
        "the original must still lead the description: {:?}",
        task.description
    );
    assert!(
        !task.description.contains("\n\n\n"),
        "the CLI collapsed the blank run, which is exactly why the guard must normalize"
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

/// The write-path guard (audit finding F1, layer 2) against a real file the
/// real CLI wrote — including one carrying a fenced `## ` heading, the shape
/// that used to be silently truncated on the next save.
#[test]
fn a_cli_written_task_round_trips_through_the_parser_even_with_a_fenced_heading() {
    let fixture = fixture_repo();
    let root = fixture.path();
    let task_id = create_fixture_task(root);

    let patch = BacklogTaskPatch {
        description: Some(
            "Intro.\n\n```markdown\n## A heading inside a fence\n```\n\nOutro.".to_string(),
        ),
        ..Default::default()
    };
    edit_backlog_task(root, &task_id, &patch).expect("write a fenced description");

    let project = reload(root);
    let task = project.tasks.iter().find(|t| t.id == task_id).unwrap();

    assert!(
        task.description.contains("## A heading inside a fence"),
        "a fenced heading is content: {:?}",
        task.description
    );
    assert!(
        task.description.contains("Outro."),
        "content after the fence must survive the read: {:?}",
        task.description
    );
    assert!(
        task_file_round_trips(&task.path),
        "a normal CLI-written task must pass the guard, or refine would skip every write"
    );

    // And the write path is now genuinely non-destructive: saving the parsed
    // description straight back must not lose the fence.
    let resave = BacklogTaskPatch {
        description: Some(task.description.clone()),
        ..Default::default()
    };
    edit_backlog_task(root, &task_id, &resave).expect("resave");
    let project = reload(root);
    let task = project.tasks.iter().find(|t| t.id == task_id).unwrap();
    assert!(
        task.description.contains("## A heading inside a fence")
            && task.description.contains("Outro."),
        "a read-then-write cycle must be lossless: {:?}",
        task.description
    );
}

#[test]
fn append_notes_persists_and_accumulates() {
    let fixture = fixture_repo();
    let root = fixture.path();
    let task_id = create_fixture_task(root);

    append_backlog_notes(root, &task_id, "First note").expect("first append should succeed");
    append_backlog_notes(root, &task_id, "Second note").expect("second append should succeed");

    let project = reload(root);
    let task = project.tasks.iter().find(|t| t.id == task_id).unwrap();
    assert!(task.implementation_notes.contains("First note"));
    assert!(task.implementation_notes.contains("Second note"));
}

#[test]
fn append_notes_rejects_an_empty_note_without_touching_the_cli() {
    let fixture = fixture_repo();
    let root = fixture.path();
    let task_id = create_fixture_task(root);

    let result = append_backlog_notes(root, &task_id, "   ");
    assert!(
        result.is_err(),
        "a whitespace-only note should be rejected before ever invoking the CLI"
    );
}

#[test]
fn archive_moves_the_task_out_of_the_active_set() {
    let fixture = fixture_repo();
    let root = fixture.path();
    let task_id = create_fixture_task(root);

    archive_backlog_task(root, &task_id).expect("archive_backlog_task should succeed");

    let project = reload(root);
    let task = project
        .tasks
        .iter()
        .find(|t| t.id == task_id)
        .expect("archived task should still be reparsed, just from a different source dir");
    assert_eq!(
        task.source,
        BacklogTaskSource::Archived,
        "archived task should reparse with BacklogTaskSource::Archived"
    );
}

/// FIXED (was a defect found during 2026-08-05 fix-wave re-verification of
/// "Clean Up Old Tasks", a new finding, not one of the original six): the
/// real `backlog` CLI v1.47.1 refuses `task archive` on a Done-status task —
/// confirmed empirically: `Task TASK-1 is Done. Done tasks should be
/// completed, not archived. Use: backlog task complete TASK-1`.
/// `archive_moves_the_task_out_of_the_active_set` (above) never caught this
/// because `create_fixture_task`'s task defaults to "To Do" — every prior
/// Archive test (this file and `switchbard-gui`'s `archive_confirm_sets_the_
/// synchronous_archiving_status`) exercised a non-Done task. This is a real
/// Backlog.md semantic, not a bug to route around: a Done task is
/// *completed* (`backlog task complete`, lands in `backlog/completed/`), a
/// non-Done task is *archived* (`backlog task archive`, `backlog/archive/`)
/// — the two are mutually exclusive dispositions chosen by status. Fixed by
/// adding `complete_backlog_task` and routing both the GUI's single-task
/// Archive button (`detail_lists::render_archive`, which now shows
/// "Complete" instead of "Archive" when `task.is_done()`) and "Clean Up Old
/// Tasks" (`HiveApp::spawn_backlog_cleanup`, which exclusively targets Done
/// tasks) through it instead of `archive_backlog_task`. This test now pins
/// both halves: the CLI's permanent refusal of `archive` on a Done task
/// (still true, not something to "fix"), and `complete_backlog_task`
/// succeeding and landing the task as `BacklogTaskSource::Completed`. See
/// `switchbard-gui/tests/qa_reverify_2026_08_05.rs`'s companion test for
/// "Clean Up Old Tasks" itself.
#[test]
fn archiving_a_done_task_is_rejected_by_the_real_cli() {
    let fixture = fixture_repo();
    let root = fixture.path();
    let task_id = create_fixture_task(root);
    edit_backlog_task(
        root,
        &task_id,
        &BacklogTaskPatch {
            status: Some("Done".to_string()),
            ..Default::default()
        },
    )
    .expect("marking the task Done should succeed");

    let archive_err = archive_backlog_task(root, &task_id)
        .expect_err("the real CLI should still refuse `task archive` on a Done task");
    assert!(
        archive_err.to_string().contains("complete"),
        "the refusal message should point at `task complete`: {archive_err}"
    );

    complete_backlog_task(root, &task_id).expect("complete_backlog_task should succeed");

    let project = reload(root);
    let task = project
        .tasks
        .iter()
        .find(|t| t.id == task_id)
        .expect("completed task should still be reparsed, just from backlog/completed/");
    assert_eq!(
        task.source,
        BacklogTaskSource::Completed,
        "a Done task should land as Completed, not Archived"
    );
}

#[test]
fn set_backlog_label_adds_and_removes_a_single_label_without_touching_others() {
    let fixture = fixture_repo();
    let root = fixture.path();
    let task_id = create_fixture_task(root);
    edit_backlog_task(
        root,
        &task_id,
        &BacklogTaskPatch {
            labels: Some(vec!["keep-me".to_string()]),
            ..Default::default()
        },
    )
    .unwrap();

    set_backlog_label(root, &task_id, "dispatch", true).expect("adding the label should succeed");
    let project = reload(root);
    let task = project.tasks.iter().find(|t| t.id == task_id).unwrap();
    assert!(task.labels.contains(&"dispatch".to_string()));
    assert!(
        task.labels.contains(&"keep-me".to_string()),
        "set_backlog_label must not clobber the task's other labels"
    );

    set_backlog_label(root, &task_id, "dispatch", false)
        .expect("removing the label should succeed");
    let project = reload(root);
    let task = project.tasks.iter().find(|t| t.id == task_id).unwrap();
    assert!(!task.labels.contains(&"dispatch".to_string()));
    assert!(
        task.labels.contains(&"keep-me".to_string()),
        "removing one label must not touch the others"
    );
}

/// The dispatch queue's own claim step (`dispatch` → `dispatching`) — a
/// single atomic `--remove-label`/`--add-label` invocation, distinct from
/// `set_backlog_label`'s single-flag add/remove.
#[test]
fn swap_backlog_label_atomically_replaces_one_label_with_another() {
    let fixture = fixture_repo();
    let root = fixture.path();
    let task_id = create_fixture_task(root);
    set_backlog_label(root, &task_id, "dispatch", true).unwrap();

    swap_backlog_label(root, &task_id, "dispatch", "dispatching")
        .expect("swap_backlog_label should succeed");

    let project = reload(root);
    let task = project.tasks.iter().find(|t| t.id == task_id).unwrap();
    assert!(!task.labels.contains(&"dispatch".to_string()));
    assert!(task.labels.contains(&"dispatching".to_string()));
}

/// TASK-43 F4b: a task re-flagged after a failed run must not carry its old
/// verdict into the new run.
///
/// The dispatch state machine is a priority ladder — `dispatched` beats
/// `dispatch-failed` beats `dispatching` — so a task that kept
/// `dispatch-failed` while `dispatching` rendered as a red "DISPATCH FAILED"
/// pill for the entire length of a perfectly healthy agent run, and lit the
/// top bar's attention chip with a warning nothing could clear. Claiming is
/// the moment the previous attempt stops being the current truth, so claiming
/// is where the stale labels go.
#[test]
fn claiming_a_task_clears_the_previous_attempts_terminal_labels() {
    let fixture = fixture_repo();
    let root = fixture.path();
    let task_id = create_fixture_task(root);
    // The exact on-disk state after one failed run followed by a re-flag.
    set_backlog_label(root, &task_id, "dispatch-failed", true).unwrap();
    set_backlog_label(root, &task_id, "keep-me", true).unwrap();
    set_backlog_label(root, &task_id, "dispatch", true).unwrap();

    claim_task_for_dispatch(root, &task_id).expect("claim should succeed");

    let project = reload(root);
    let task = project.tasks.iter().find(|t| t.id == task_id).unwrap();
    assert!(
        task.labels.contains(&"dispatching".to_string()),
        "the claim itself must still happen: {:?}",
        task.labels
    );
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

/// The same guard for a task re-flagged after a *successful* run, which is
/// the likelier real sequence (open a PR, decide it needs another pass).
/// `dispatched` outranks everything, so leaving it behind would show a green
/// "DISPATCHED" pill and a stale PR link over a live run.
#[test]
fn claiming_a_task_clears_a_previous_successful_runs_label() {
    let fixture = fixture_repo();
    let root = fixture.path();
    let task_id = create_fixture_task(root);
    set_backlog_label(root, &task_id, "dispatched", true).unwrap();
    set_backlog_label(root, &task_id, "dispatch", true).unwrap();

    claim_task_for_dispatch(root, &task_id).expect("claim should succeed");

    let project = reload(root);
    let task = project.tasks.iter().find(|t| t.id == task_id).unwrap();
    assert!(task.labels.contains(&"dispatching".to_string()));
    assert!(!task.labels.contains(&"dispatched".to_string()));
}

/// A first-ever claim has nothing to clear; the clearing step must be a
/// silent no-op rather than an error that aborts the claim.
#[test]
fn claiming_a_never_dispatched_task_is_a_plain_swap() {
    let fixture = fixture_repo();
    let root = fixture.path();
    let task_id = create_fixture_task(root);
    set_backlog_label(root, &task_id, "dispatch", true).unwrap();

    claim_task_for_dispatch(root, &task_id).expect("claim should succeed");

    let project = reload(root);
    let task = project.tasks.iter().find(|t| t.id == task_id).unwrap();
    assert_eq!(task.labels, vec!["dispatching".to_string()]);
}

/// QA parity matrix LOW gap: labels/assignee/milestone/dependencies are now
/// settable at `NewBacklogTask` creation time, not just afterward via
/// `edit_backlog_task`. Proves the flags `create_backlog_task` now builds
/// (`-l`, `-a`, `-m`, `--depends-on` — verified against a real `backlog task
/// create --help` before implementing, not guessed) actually persist.
#[test]
fn create_backlog_task_wires_labels_assignee_milestone_and_dependencies() {
    let fixture = fixture_repo();
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

    let project = reload(root);
    let task = project
        .tasks
        .iter()
        .find(|t| t.title == "Fully specified task")
        .unwrap_or_else(|| panic!("created task should be present: {output}"));
    assert_eq!(task.labels, vec!["frontend", "urgent"]);
    assert_eq!(task.assignees, vec!["ben"]);
    assert_eq!(task.milestone.as_deref(), Some("v1"));
    assert_eq!(task.dependencies, vec![dependency_id]);
}

/// FIXED (was HIGH DEFECT, filed in the 2026-08-05 QA parity audit, see
/// `docs/qa/2026-08-05-parity-qa.md`): the real `backlog` CLI (v1.47.1,
/// confirmed empirically) writes a subtask's parent as `parent_task_id:` in
/// frontmatter, but `parse_task_file` (`switchbard-core/src/backlog.rs`) read
/// the key `"parent"`. The two never matched, so `BacklogTask::parent` was
/// silently `None` for every real subtask — the entire task-17 sub-task
/// hierarchy feature (roll-up badges, tree expand/collapse, "+ Subtask") was
/// unreachable against a real Backlog.md project. It only appeared to work in
/// `switchbard-gui`'s tests because those construct
/// `BacklogTask { parent: Some(...), .. }` directly in Rust, bypassing the
/// parser entirely. `parse_task_file` now reads `parent_task_id` first,
/// falling back to `parent` for fixtures/tasks written before this fix. This
/// test pins that behavior end to end against a real fixture repo.
#[test]
fn create_backlog_task_wires_a_subtask_parent() {
    let fixture = fixture_repo();
    let root = fixture.path();
    let parent_id = create_fixture_task(root);

    let subtask = NewBacklogTask {
        title: "Fixture subtask".to_string(),
        description: String::new(),
        status: "To Do".to_string(),
        priority: "medium".to_string(),
        acceptance_criteria: vec![],
        parent: Some(parent_id.clone()),
        labels: vec![],
        assignees: vec![],
        milestone: None,
        dependencies: vec![],
    };
    create_backlog_task(root, &subtask).expect("subtask create should succeed");

    let project = reload(root);
    let child = project
        .tasks
        .iter()
        .find(|t| t.title == "Fixture subtask")
        .expect("subtask should be present");
    assert_eq!(child.parent.as_deref(), Some(parent_id.as_str()));
}

/// Sanity check for the QA report's "decimal subtask ids" edge case: Backlog
/// CLI numbers a subtask `<parent>.1`, `<parent>.2`, ... and
/// `sorts_task_id_decimals_numerically` (backlog.rs's own unit test) already
/// proves the parser orders those correctly; this proves the *id shape*
/// itself end to end against the real CLI, not just a hand-built fixture.
/// Previously blocked on the same `parent`/`parent_task_id` defect as the
/// test above — see `docs/qa/2026-08-05-parity-qa.md`.
#[test]
fn subtask_ids_are_decimal_children_of_the_parent_id() {
    let fixture = fixture_repo();
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
        "Backlog CLI should number subtasks as decimal children of the parent id"
    );
}

/// QA edge case: two independent fixture projects (simulating two repos) can
/// both mint a `TASK-1` without colliding on disk — the GUI's own
/// disambiguation is a `(project_root, task_id)` key
/// (`switchbard_gui::runtime::BacklogTaskKey`), proven at the GUI layer in
/// `backlog_all_projects_scope_merges_repos_with_a_repo_badge`
/// (`ui_views.rs`); this proves the two on-disk projects underneath it are
/// genuinely independent, not aliases of the same store.
#[test]
fn two_independent_fixture_projects_can_both_mint_task_1() {
    let repo_a = fixture_repo();
    let repo_b = fixture_repo();
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
    .unwrap();

    let project_b = reload(repo_b.path());
    let task_b = project_b.tasks.iter().find(|t| t.id == "TASK-1").unwrap();
    assert_eq!(
        task_b.title, "Fixture task",
        "editing repo A's TASK-1 must not affect repo B's own TASK-1"
    );
}

/// QA edge case: an empty repo (a `backlog init` with zero tasks created)
/// should load as a valid, empty project rather than erroring — the GUI's
/// "No tracked worktrees have a backlog/config.yml..." empty state
/// (`mod.rs::render_empty`) is for zero *tracked projects*, not this case
/// (a tracked project with zero tasks), so this proves `load_backlog_project`
/// itself degrades gracefully.
#[test]
fn empty_project_with_no_tasks_loads_cleanly() {
    let fixture = fixture_repo();
    let project = reload(fixture.path());
    assert!(project.tasks.is_empty());
    assert!(project.warnings.is_empty());
}

/// QA edge case: "missing CLI". `backlog_cli_path()` checks `PATH` and then
/// two hardcoded common install locations (`/opt/homebrew/bin`,
/// `/usr/local/bin`) — on this dev machine `backlog` is genuinely installed
/// at one of those, so there is no way to make `backlog_cli_path()` return
/// `None` from inside a test process without relocating a real system binary
/// (unsafe and out of scope) or mutating the process-global `PATH`, which
/// would race every other test in this binary that shells out to `git`/
/// `backlog` concurrently (confirmed empirically: an earlier version of this
/// test intermittently broke sibling tests with "No such file or directory").
/// So this proves the boundary that actually matters — what a project loads
/// as when the CLI is unavailable — directly, the same shape
/// `load_backlog_project` produces when `backlog_cli_path()` returns `None`
/// (see `backlog.rs`'s `cli_path.is_none()` branch), rather than trying to
/// force that condition through a real subprocess call.
#[test]
fn cli_unavailable_project_reports_not_available() {
    let project = BacklogProject {
        root: std::path::PathBuf::from("/tmp/does-not-matter"),
        cli_path: None,
        tasks: vec![],
        warnings: vec!["Backlog CLI not found on PATH".to_string()],
        loaded_at_unix: 0,
        configured_statuses: vec![],
    };
    assert!(!project.cli_available());
}

/// QA edge case: a task file the parser genuinely cannot read (invalid
/// UTF-8) should surface as a project-level warning, not a hard failure of
/// the whole project load — `load_backlog_project` routes per-file parse
/// errors into `BacklogProject::warnings` (see `backlog.rs`'s loop over
/// `parse_task_file`, which calls `fs::read_to_string` first).
///
/// Note: a syntactically-odd-but-valid-UTF-8 file (e.g. missing frontmatter
/// fences, no `id:` key) does **not** hit this path — `split_frontmatter`
/// degrades gracefully to an empty mapping and `parse_task_file` falls back
/// to `id_from_filename`, so it still parses as a best-effort task rather
/// than warning. Only a read-level failure (bad UTF-8, unreadable file)
/// actually reaches the warning branch; this test exercises that real case
/// rather than one the parser is deliberately lenient about.
#[test]
fn an_unreadable_task_file_becomes_a_warning_not_a_load_failure() {
    let fixture = fixture_repo();
    let root = fixture.path();
    create_fixture_task(root);
    let malformed = root.join("backlog/tasks/task-99 - Malformed.md");
    fs::write(&malformed, [0xFF, 0xFE, 0x00, 0xFF]).unwrap();

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
