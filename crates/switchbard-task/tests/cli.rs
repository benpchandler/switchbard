//! End-to-end tests driving the real `switchbard-task` binary against real
//! on-disk fixture projects — the output contract the help text promises,
//! proven at the process boundary (exit codes, stdout payload purity,
//! stderr error shape), not just at the library layer `switchbard-core`'s
//! own tests already cover.

use std::path::Path;
use std::process::{Command, Output};

fn fixture_project() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::create_dir_all(dir.path().join("backlog/tasks")).expect("layout");
    std::fs::write(
        dir.path().join("backlog/config.yml"),
        "statuses: [\"To Do\", \"In Progress\", \"Done\"]\n",
    )
    .expect("config");
    dir
}

fn bin(root: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_switchbard-task"))
        .arg("--repo")
        .arg(root)
        .args(args)
        .output()
        .expect("binary runs")
}

fn ok_stdout(root: &Path, args: &[&str]) -> String {
    let out = bin(root, args);
    assert!(
        out.status.success(),
        "`switchbard-task {args:?}` failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).expect("stdout is utf-8")
}

#[test]
fn create_prints_only_the_id_and_the_lifecycle_round_trips() {
    let dir = fixture_project();
    let root = dir.path();

    let id = ok_stdout(
        root,
        &[
            "create",
            "Ship the fork",
            "-d",
            "Why and what.",
            "--ac",
            "First",
            "--ac",
            "Second",
            "-l",
            "fork,cli",
        ],
    );
    assert_eq!(id, "TASK-1\n", "create's stdout is the id alone");

    // Work the task the way the lifecycle does.
    assert_eq!(
        ok_stdout(
            root,
            &["edit", "TASK-1", "-s", "In Progress", "-a", "agent"]
        ),
        "Edited TASK-1\n"
    );
    assert_eq!(
        ok_stdout(root, &["edit", "1", "--check-ac", "1", "--check-ac", "2"]),
        "Edited 1\n",
        "bare numeric ids work"
    );
    assert_eq!(
        ok_stdout(
            root,
            &["edit", "TASK-1", "--append-notes", "Did the thing."]
        ),
        "Edited TASK-1\n"
    );
    assert_eq!(
        ok_stdout(
            root,
            &["edit", "TASK-1", "--final-summary", "Shipped end to end."]
        ),
        "Edited TASK-1\n"
    );
    assert_eq!(
        ok_stdout(root, &["edit", "TASK-1", "-s", "Done"]),
        "Edited TASK-1\n"
    );

    let view = ok_stdout(root, &["view", "task-1"]);
    assert!(view.starts_with("TASK-1 - Ship the fork\n"), "{view}");
    assert!(view.contains("Status: Done\n"));
    assert!(view.contains("- [x] #1 First\n"));
    assert!(view.contains("- [x] #2 Second\n"));
    assert!(view.contains("\n## Implementation Notes\n\nDid the thing.\n"));
    assert!(view.contains("\n## Final Summary\n\nShipped end to end.\n"));

    assert_eq!(
        ok_stdout(root, &["complete", "TASK-1"]),
        "Completed task TASK-1\n"
    );
    let listed = ok_stdout(root, &["list", "--all"]);
    assert!(listed.contains("TASK-1\tDone\t"), "{listed}");
    assert_eq!(
        ok_stdout(root, &["list"]),
        "",
        "a completed task leaves the active list"
    );
}

#[test]
fn list_rows_are_tab_separated_and_status_filterable() {
    let dir = fixture_project();
    let root = dir.path();
    let first = ok_stdout(root, &["create", "First task", "-l", "alpha"]);
    assert_eq!(first, "TASK-1\n");
    assert_eq!(ok_stdout(root, &["create", "Second task"]), "TASK-2\n");
    assert_eq!(
        ok_stdout(root, &["edit", "TASK-2", "-s", "In Progress"]),
        "Edited TASK-2\n"
    );

    assert_eq!(
        ok_stdout(root, &["list"]),
        "TASK-2\tIn Progress\tmedium\t\t\tSecond task\n\
         TASK-1\tTo Do\tmedium\talpha\t\tFirst task\n",
        "rows sort like the loader (In Progress first) and carry 6 columns"
    );
    assert_eq!(
        ok_stdout(root, &["list", "--status", "to do"]),
        "TASK-1\tTo Do\tmedium\talpha\t\tFirst task\n",
        "status filter is case-insensitive"
    );
}

#[test]
fn dispatch_flagging_is_a_single_label_toggle() {
    let dir = fixture_project();
    let root = dir.path();
    assert_eq!(ok_stdout(root, &["create", "Agent bait"]), "TASK-1\n");

    assert_eq!(
        ok_stdout(root, &["edit", "TASK-1", "--add-label", "dispatch"]),
        "Edited TASK-1\n"
    );
    assert!(ok_stdout(root, &["view", "TASK-1"]).contains("Labels: dispatch\n"));
    assert_eq!(
        ok_stdout(root, &["edit", "TASK-1", "--add-label", "dispatch"]),
        "no changes\n",
        "re-flagging is an honest no-op"
    );
    assert_eq!(
        ok_stdout(root, &["edit", "TASK-1", "--remove-label", "dispatch"]),
        "Edited TASK-1\n"
    );
}

#[test]
fn errors_are_one_stderr_line_with_a_next_step_and_exit_code_one() {
    let dir = fixture_project();
    let root = dir.path();
    assert_eq!(ok_stdout(root, &["create", "Lonely"]), "TASK-1\n");

    let missing = bin(root, &["view", "TASK-9"]);
    assert_eq!(missing.status.code(), Some(1));
    assert!(missing.stdout.is_empty(), "errors never touch stdout");
    let err = String::from_utf8_lossy(&missing.stderr);
    assert!(
        err.starts_with("switchbard-task: error: no task TASK-9"),
        "{err}"
    );
    assert!(
        err.contains("switchbard-task list"),
        "errors carry a next step"
    );

    let bad_status = bin(root, &["edit", "TASK-1", "-s", "Blocked"]);
    assert_eq!(bad_status.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&bad_status.stderr)
        .contains("Invalid status: Blocked. Valid statuses are: To Do, In Progress, Done"));

    let done_refuses = bin(root, &["complete", "TASK-1"]);
    assert_eq!(done_refuses.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&done_refuses.stderr).contains("archive it instead"));
}

#[test]
fn outside_a_project_the_error_names_the_escape_hatch() {
    let dir = tempfile::tempdir().expect("tempdir");
    let out = Command::new(env!("CARGO_BIN_EXE_switchbard-task"))
        .current_dir(dir.path())
        .args(["list"])
        .output()
        .expect("binary runs");

    assert_eq!(out.status.code(), Some(1));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("no Backlog repo"), "{err}");
    assert!(
        err.contains("--repo"),
        "the error names the escape hatch: {err}"
    );
}

#[test]
fn deprecated_project_flag_still_works_and_warns_on_stderr_only() {
    let dir = fixture_project();
    let out = Command::new(env!("CARGO_BIN_EXE_switchbard-task"))
        .arg("--project")
        .arg(dir.path())
        .args(["create", "Via legacy flag"])
        .output()
        .expect("binary runs");

    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    // Payload purity: the id is still the only thing on stdout.
    assert_eq!(String::from_utf8_lossy(&out.stdout), "TASK-1\n");
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("--project is deprecated; use --repo"), "{err}");
}

#[test]
fn subtask_create_mints_a_decimal_child_id() {
    let dir = fixture_project();
    let root = dir.path();
    assert_eq!(ok_stdout(root, &["create", "Parent"]), "TASK-1\n");

    assert_eq!(
        ok_stdout(root, &["create", "Child", "-p", "TASK-1"]),
        "TASK-1.1\n"
    );
    let view = ok_stdout(root, &["view", "1.1"]);
    assert!(view.contains("Parent: TASK-1\n"), "{view}");
}

#[test]
fn in_project_assignment_round_trips_and_filters_the_list() {
    let dir = fixture_project();
    let root = dir.path();
    assert_eq!(
        ok_stdout(root, &["create", "In the cutover", "-m", "Lucella cutover"]),
        "TASK-1\n"
    );
    assert_eq!(ok_stdout(root, &["create", "Elsewhere"]), "TASK-2\n");

    let listed = ok_stdout(root, &["list"]);
    assert!(
        listed.contains("TASK-1\tTo Do\tmedium\t\tLucella cutover\tIn the cutover\n"),
        "six columns with project before title: {listed}"
    );
    assert_eq!(
        ok_stdout(root, &["list", "--in-project", "Lucella cutover"]),
        "TASK-1\tTo Do\tmedium\t\tLucella cutover\tIn the cutover\n"
    );

    let view = ok_stdout(root, &["view", "1"]);
    assert!(view.contains("Project: Lucella cutover\n"), "{view}");

    let raw = std::fs::read_to_string(root.join("backlog/tasks/task-1 - In-the-cutover.md"))
        .expect("task file readable");
    assert!(raw.contains("\nproject: Lucella cutover\n"), "{raw}");
    assert!(!raw.contains("milestone:"), "{raw}");
}

#[test]
fn legacy_milestone_flags_still_work_and_migrate_the_key() {
    let dir = fixture_project();
    let root = dir.path();
    // A pre-divergence file written with the old key.
    std::fs::write(
        root.join("backlog/tasks/task-1 - Legacy.md"),
        "---\nid: TASK-1\ntitle: Legacy\nstatus: To Do\npriority: medium\nmilestone: v1\n---\n",
    )
    .expect("fixture");

    let listed = ok_stdout(root, &["list"]);
    assert!(
        listed.contains("\tv1\tLegacy"),
        "legacy milestone: resolves as the project column: {listed}"
    );

    assert_eq!(
        ok_stdout(root, &["edit", "TASK-1", "--milestone", "v2"]),
        "Edited TASK-1\n",
        "the deprecated alias still parses"
    );
    let raw = std::fs::read_to_string(root.join("backlog/tasks/task-1 - Legacy.md"))
        .expect("task file readable");
    assert!(
        raw.contains("\nproject: v2\n"),
        "assignment migrates the key: {raw}"
    );
    assert!(!raw.contains("milestone:"), "{raw}");

    assert_eq!(
        ok_stdout(root, &["edit", "TASK-1", "--clear-milestone"]),
        "Edited TASK-1\n"
    );
    let raw = std::fs::read_to_string(root.join("backlog/tasks/task-1 - Legacy.md"))
        .expect("task file readable");
    assert!(!raw.contains("project:"), "{raw}");
}

#[test]
fn project_family_round_trips_with_rollup_and_honest_errors() {
    let dir = fixture_project();
    let root = dir.path();

    assert_eq!(
        ok_stdout(
            root,
            &[
                "project",
                "create",
                "Lucella cutover",
                "-d",
                "Make lucella.app canonical.",
                "--target-date",
                "2026-10-01",
                "--initiative",
                "Rebrand",
            ]
        ),
        "Lucella cutover\n",
        "create prints the name alone"
    );

    assert_eq!(
        ok_stdout(root, &["create", "Member", "-m", "Lucella cutover"]),
        "TASK-1\n"
    );
    assert_eq!(
        ok_stdout(root, &["edit", "TASK-1", "-s", "Done"]),
        "Edited TASK-1\n"
    );

    assert_eq!(
        ok_stdout(root, &["project", "list"]),
        "Lucella cutover\tPlanned\t1/1\t100%\t2026-10-01\tRebrand\n"
    );

    let view = ok_stdout(root, &["project", "view", "Lucella cutover"]);
    assert!(view.starts_with("Lucella cutover\n"), "{view}");
    assert!(view.contains("Progress: 1/1 (100%)\n"), "{view}");
    assert!(view.contains("Make lucella.app canonical."), "{view}");
    assert!(
        view.contains("TASK-1\tDone\t"),
        "member tasks render as list rows: {view}"
    );

    assert_eq!(
        ok_stdout(
            root,
            &["project", "edit", "Lucella cutover", "-s", "In Progress"]
        ),
        "Edited Lucella cutover\n"
    );
    assert_eq!(
        ok_stdout(
            root,
            &["project", "edit", "Lucella cutover", "-s", "In Progress"]
        ),
        "no changes\n"
    );
    assert_eq!(
        ok_stdout(root, &["project", "complete", "Lucella cutover"]),
        "Completed Lucella cutover\n"
    );

    let bad_status = bin(
        root,
        &["project", "edit", "Lucella cutover", "-s", "Shipped"],
    );
    assert_eq!(bad_status.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&bad_status.stderr).contains(
        "Invalid status: Shipped. Valid statuses are: Planned, In Progress, Completed, Canceled"
    ));

    let undefined = bin(root, &["project", "complete", "Ghost"]);
    assert_eq!(undefined.status.code(), Some(1));
    assert!(
        String::from_utf8_lossy(&undefined.stderr).contains("project create"),
        "the error names the next step"
    );
}

#[test]
fn initiative_family_rolls_up_member_projects() {
    let dir = fixture_project();
    let root = dir.path();
    assert_eq!(
        ok_stdout(
            root,
            &["initiative", "create", "Rebrand", "-s", "In Progress"]
        ),
        "Rebrand\n"
    );
    assert_eq!(
        ok_stdout(
            root,
            &["project", "create", "Alpha", "--initiative", "Rebrand"]
        ),
        "Alpha\n"
    );
    assert_eq!(
        ok_stdout(root, &["create", "Work", "-m", "Alpha"]),
        "TASK-1\n"
    );

    assert_eq!(
        ok_stdout(root, &["initiative", "list"]),
        "Rebrand\tIn Progress\t0/1\t0%\t\t1\n"
    );
    let view = ok_stdout(root, &["initiative", "view", "Rebrand"]);
    assert!(view.contains("Progress: 0/1 (0%)\n"), "{view}");
    assert!(view.contains("Alpha\tPlanned\t0/1\t0%"), "{view}");

    assert_eq!(
        ok_stdout(root, &["initiative", "archive", "Rebrand"]),
        "Canceled Rebrand\n"
    );
}
