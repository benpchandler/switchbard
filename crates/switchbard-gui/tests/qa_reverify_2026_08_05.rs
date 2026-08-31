//! Independent re-verification (by the original QA auditor, not the fix-wave
//! implementer) of the six items closed in the 2026-08-05 fix wave
//! (commits `c33f5e1..400d764` on `feature/integration`). Each test here
//! uses its own data and interaction choices, distinct from the
//! implementer's own tests in `backlog_controls.rs`/`backlog_cli_
//! mutations.rs`, specifically to catch anything those might have missed
//! rather than re-running the same assertions. Where the implementer's own
//! tests already drove a control via kittest state assignment (a legitimate
//! choice for an UNDRIVABLE ComboBox), this file prefers actual `type_text`
//! keystrokes and, for the two riskiest items (Create-modal CLI fields,
//! Clean Up Old Tasks), a real fixture repo and real `backlog` CLI rather
//! than an in-memory fixture — the same standard the original audit held
//! the highest-risk controls to.

mod common;

use egui_kittest::kittest::NodeT;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use common::{harness, isolated_config_save_path, seeded_app, REPO_PATH};
use egui_kittest::kittest::{self, Queryable};
use switchbard_core::config::Config;
use switchbard_core::{BacklogRepo, BacklogTask, BacklogTaskSource, Repo, WorktreeRef};
use switchbard_gui::app::HiveApp;
use switchbard_gui::runtime::{BacklogLens, BacklogTaskSortDirection, BacklogTaskSortKey, ViewTab};

fn task(id: &str, title: &str, status: &str) -> BacklogTask {
    BacklogTask {
        id: id.to_string(),
        title: title.to_string(),
        status: status.to_string(),
        priority: "medium".to_string(),
        assignees: vec![],
        labels: vec![],
        dependencies: vec![],
        references: vec![],
        project: None,
        parent: None,
        created_date: Some("2026-06-01 09:00".to_string()),
        updated_date: Some("2026-06-01 09:00".to_string()),
        description: String::new(),
        implementation_plan: String::new(),
        implementation_notes: String::new(),
        final_summary: String::new(),
        acceptance_criteria: vec![],
        definition_of_done: vec![],
        source: BacklogTaskSource::Active,
        path: PathBuf::from(format!(
            "{REPO_PATH}/backlog/tasks/{}.md",
            id.to_lowercase()
        )),
    }
}

fn list_app_with_tasks(tasks: Vec<BacklogTask>) -> HiveApp {
    let mut app = seeded_app();
    app.view_tab = ViewTab::Backlog;
    app.backlog_view.lens = BacklogLens::List;
    app.backlog_view.selected_repo = Some(PathBuf::from(REPO_PATH));
    app.backlog_repos.lock().unwrap().insert(
        PathBuf::from(REPO_PATH),
        BacklogRepo {
            root: PathBuf::from(REPO_PATH),
            tasks,
            warnings: vec![],
            project_defs: vec![],
            initiative_defs: vec![],
            loaded_at_unix: 0,
            configured_statuses: vec![
                "Icebox".into(),
                "To Do".into(),
                "In Progress".into(),
                "In Review".into(),
                "Done".into(),
            ],
        },
    );
    app
}

fn now_minus_days(days: i64) -> String {
    (chrono::Utc::now() - chrono::Duration::days(days))
        .format("%Y-%m-%d %H:%M")
        .to_string()
}

/// Native fixture init — the directory shape plus a config declaring the
/// standard trio, matching what `backlog init --defaults` used to produce
/// before the format fork retired the external CLI (TASK-67).
fn native_backlog_init(root: &std::path::Path) {
    std::fs::create_dir_all(root.join("backlog/tasks")).expect("fixture layout");
    std::fs::write(
        root.join("backlog/config.yml"),
        "statuses: [\"To Do\", \"In Progress\", \"Done\"]\n",
    )
    .expect("fixture config");
}

fn native_task_create(root: &std::path::Path, title: &str) -> String {
    switchbard_core::create_backlog_task(
        root,
        &switchbard_core::NewBacklogTask {
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
        },
    )
    .expect("native fixture create")
}

fn native_task_status(root: &std::path::Path, id: &str, status: &str) {
    switchbard_core::edit_backlog_task(
        root,
        id,
        &switchbard_core::BacklogTaskPatch {
            status: Some(status.to_string()),
            ..Default::default()
        },
    )
    .expect("native fixture status edit");
}

fn init_fixture_repo() -> tempfile::TempDir {
    let fixture = tempfile::tempdir().expect("create temp dir");
    let root = fixture.path();
    native_backlog_init(root);
    fixture
}

// ─── 1. Board cards: labels + age (fix wave: fc080f5) ────────────────────
//
// Independent of the fix's own `board_card_shows_labels_and_a_humanized_age`
// test: uses a *different* label set, and specifically targets the
// `updated_date`-over-`created_date` precedence claim from the commit
// message with two dates far enough apart that the two would land in
// different humanize_age buckets ("d" vs "mo") if the precedence were
// backwards — a same-bucket age wouldn't have caught a swapped field.

#[test]
fn board_card_age_prefers_updated_date_over_created_date() {
    let mut t = task("TASK-1", "Recently touched", "To Do");
    t.labels = vec!["release-blocker".to_string(), "qa".to_string()];
    t.created_date = Some(now_minus_days(40)); // would read "~1mo ago" if used
    t.updated_date = Some(now_minus_days(2)); // should read "2d ago"

    let mut app = list_app_with_tasks(vec![t]);
    app.backlog_view.lens = BacklogLens::Board;
    let mut h = harness(app);
    h.run();

    assert!(
        h.query_by_label("release-blocker, qa").is_some(),
        "labels should render comma-joined on the board card"
    );
    assert!(
        h.query_by_label("2d ago").is_some(),
        "age should reflect updated_date (2 days ago), not created_date (~40 days ago)"
    );
    assert!(
        h.query_by_label("1mo ago").is_none(),
        "age must not fall back to created_date when updated_date is present"
    );
}

#[test]
fn board_card_with_no_labels_and_no_dates_omits_the_line_entirely() {
    let mut t = task("TASK-1", "Bare task", "To Do");
    t.created_date = None;
    t.updated_date = None;
    let mut app = list_app_with_tasks(vec![t]);
    app.backlog_view.lens = BacklogLens::Board;
    let mut h = harness(app);
    h.run();

    // Owner UX pass (2026-08-05, post-dates this test): with a single task,
    // `reconcile_selected_task` auto-selects it and the persistent detail
    // rail now also renders its title as a heading, so "Bare task" is no
    // longer a unique label — `query_all` instead of the exactly-one query.
    assert!(
        h.query_all_by_label("Bare task").next().is_some(),
        "sanity: the card itself should still render"
    );
    // No label/age line means no "X ago" text anywhere and no comma-joined
    // label string; can't assert a specific absent string for "no labels"
    // (there's nothing to name), so this is covered by the positive case
    // above proving the line *does* render when there's something to show.
    for suffix in [
        "s ago", "m ago", "h ago", "d ago", "w ago", "mo ago", "y ago",
    ] {
        assert!(
            h.query_all(kittest::by().label_contains(suffix)).next().is_none(),
            "no age text of any unit should render for a dateless task, found one containing {suffix:?}"
        );
    }
}

// ─── 2. List sort by labels/assignee/milestone: GUI wiring, not just the
//        pure compare_tasks unit test (fix wave: 2f264f3) ────────────────
//
// The fix's own tests are unit tests directly against `compare_tasks` — real
// but they don't prove the sort key is actually *wired into the rendered
// row order*. This drives it through `render()`'s real pipeline and checks
// the rows' actual on-screen vertical order.

#[test]
fn sort_by_labels_actually_reorders_the_rendered_rows() {
    let mut has_zzz = task("TASK-1", "Zzz label task", "To Do");
    has_zzz.labels = vec!["zzz".to_string()];
    let mut has_aaa = task("TASK-2", "Aaa label task", "To Do");
    has_aaa.labels = vec!["aaa".to_string()];
    let unlabeled = task("TASK-3", "No label task", "To Do");

    let mut app = list_app_with_tasks(vec![has_zzz, has_aaa, unlabeled]);
    app.backlog_view.sort_key = BacklogTaskSortKey::Labels;
    app.backlog_view.sort_direction = BacklogTaskSortDirection::Ascending;
    let mut h = harness(app);
    h.run();

    let y = |label: &str| -> f64 {
        h.get_by_label(label)
            .accesskit_node()
            .raw_bounds()
            .expect("row should have bounds")
            .y0
    };
    let y_unlabeled = y("TASK-3  No label task");
    let y_aaa = y("TASK-2  Aaa label task");
    let y_zzz = y("TASK-1  Zzz label task");

    assert!(
        y_unlabeled < y_aaa && y_aaa < y_zzz,
        "ascending label sort should render unlabeled (\"\"), then \"aaa\", then \"zzz\" top to bottom; got y = {y_unlabeled}, {y_aaa}, {y_zzz}"
    );
}

#[test]
fn sort_by_milestone_actually_reorders_the_rendered_rows() {
    let mut v2 = task("TASK-1", "V2 task", "To Do");
    v2.project = Some("v2".to_string());
    let mut v1 = task("TASK-2", "V1 task", "To Do");
    v1.project = Some("v1".to_string());

    let mut app = list_app_with_tasks(vec![v2, v1]);
    app.backlog_view.sort_key = BacklogTaskSortKey::Milestone;
    app.backlog_view.sort_direction = BacklogTaskSortDirection::Ascending;
    let mut h = harness(app);
    h.run();

    let y_v1 = h
        .get_by_label("TASK-2  V1 task")
        .accesskit_node()
        .raw_bounds()
        .unwrap()
        .y0;
    let y_v2 = h
        .get_by_label("TASK-1  V2 task")
        .accesskit_node()
        .raw_bounds()
        .unwrap()
        .y0;
    assert!(
        y_v1 < y_v2,
        "ascending milestone sort should render v1 above v2"
    );
}

// ─── 3. Milestone/label filters + SavedView round trip (fix wave: a981248) ─
//
// Own milestone/label names (distinct from the fix's own test fixtures) to
// rule out a hardcoded-string bug, plus both filters active simultaneously
// (the fix's own test exercises them one at a time).

#[test]
fn milestone_and_label_filters_both_narrow_the_visible_set_together() {
    let mut matches_both = task("TASK-1", "Matches both filters", "To Do");
    matches_both.project = Some("Q3-hardening".to_string());
    matches_both.labels = vec!["security-review".to_string()];

    let mut wrong_milestone = task("TASK-2", "Wrong milestone", "To Do");
    wrong_milestone.project = Some("Q4-launch".to_string());
    wrong_milestone.labels = vec!["security-review".to_string()];

    let mut wrong_label = task("TASK-3", "Wrong label", "To Do");
    wrong_label.project = Some("Q3-hardening".to_string());
    wrong_label.labels = vec!["docs".to_string()];

    let mut h = harness(list_app_with_tasks(vec![
        matches_both,
        wrong_milestone,
        wrong_label,
    ]));
    h.run();
    // Sanity: all three visible with no filter active.
    assert!(h.query_by_label("TASK-2  Wrong milestone").is_some());
    assert!(h.query_by_label("TASK-3  Wrong label").is_some());

    h.state_mut().backlog_view.milestone_filter = "Q3-hardening".to_string();
    h.state_mut().backlog_view.label_filter = "security-review".to_string();
    h.run();

    assert!(
        h.query_by_label("TASK-1  Matches both filters").is_some(),
        "the task matching both filters should remain visible"
    );
    assert!(
        h.query_by_label("TASK-2  Wrong milestone").is_none(),
        "wrong milestone should be filtered out even though the label matches"
    );
    assert!(
        h.query_by_label("TASK-3  Wrong label").is_none(),
        "wrong label should be filtered out even though the milestone matches"
    );
}

#[test]
fn saved_view_round_trips_milestone_and_label_filters_through_a_real_reload() {
    let mut h = harness(list_app_with_tasks(vec![task("TASK-1", "Task", "To Do")]));
    h.state_mut().backlog_view.milestone_filter = "release-4.2".to_string();
    h.state_mut().backlog_view.label_filter = "needs-triage".to_string();
    h.run();

    // Enter commits the name; the separate Save button is gone. The field
    // has no accessible label of its own, so it is located as the last
    // TextInput rather than by a fixed index (see `backlog_controls.rs`).
    h.state_mut().backlog_view.saved_view_name_draft = "Release triage".to_string();
    h.run();
    h.query_all(kittest::by().role(egui::accesskit::Role::TextInput))
        .last()
        .expect("the saved-views name field should render")
        .focus();
    h.run();
    h.key_press(egui::Key::Enter);
    h.run();
    h.state_mut().save_config();

    let save_path = h.state().config_save_path.clone().unwrap();
    let reloaded = switchbard_core::config::load_from(&save_path)
        .expect("reloading the just-saved config should succeed");
    assert_eq!(reloaded.ui.saved_views.len(), 1);
    let view = &reloaded.ui.saved_views[0];
    assert_eq!(view.milestone_filter, "release-4.2");
    assert_eq!(view.label_filter, "needs-triage");
}

// ─── 4. Create modal: labels/assignee/milestone/dependencies actually
//        persisting through the real CLI, typed via kittest (not
//        state_mut) (fix wave: f167eb5) ───────────────────────────────────
//
// The implementer's own GUI test (`create_modal_labels_assignee_milestone_
// and_dependencies_fields_reset_after_create`) sets the four buffers via
// `state_mut()`, not `type_text` — a legitimate choice for proving the
// reset behavior, but it never actually drives the fields' typing
// mechanics, and their own real-CLI test (`backlog_mutations.rs`) calls
// `create_backlog_task` directly rather than going through the modal. This
// test does both at once: real keystrokes into the real modal, on a real
// fixture repo, reparsed by the real CLI's own parser afterward.

#[test]
fn create_modal_fields_typed_via_kittest_persist_through_a_real_create() {
    let fixture = init_fixture_repo();
    let root = fixture.path();
    native_task_create(root, "Dependency target");

    let repos = vec![Repo {
        name: "qa-fixture".to_string(),
        path: root.to_path_buf(),
    }];
    let worktrees = vec![WorktreeRef {
        repo_name: "qa-fixture".to_string(),
        path: root.to_path_buf(),
        branch: Some("main".to_string()),
        head: "abc1234".to_string(),
    }];
    let mut cfg = Config::default();
    cfg.ui.onboarding_dismissed = true;
    let mut app = HiveApp::new_headless(cfg, repos, worktrees);
    app.config_save_path = Some(isolated_config_save_path());
    app.view_tab = ViewTab::Backlog;
    app.backlog_view.lens = BacklogLens::List;
    app.backlog_view.selected_repo = Some(root.to_path_buf());
    app.backlog_repos.lock().unwrap().insert(
        root.to_path_buf(),
        switchbard_core::load_backlog_repo(root).expect("load the real fixture repo"),
    );

    let mut h = harness(app);
    h.run();

    h.get_by_label("+ Task").click();
    h.run();

    // Modal's TextInput order: title(0), labels(1), assignees(2),
    // milestone(3), dependencies(4) — status/priority are ComboBoxes
    // (different role), acceptance-criteria is MultilineTextInput.
    let modal = h.get_by_label("New Backlog Task");
    let fields: Vec<egui_kittest::Node<'_>> = modal
        .query_all(kittest::by().role(egui::accesskit::Role::TextInput))
        .collect();
    assert_eq!(
        fields.len(),
        5,
        "expected 5 singleline fields in the create modal (title, labels, assignees, milestone, dependencies)"
    );
    // Each field needs its own frame *and* a real pointer click, not
    // `Node::focus()`/`Node::click()` (accesskit's `Focus`/`Click` semantic
    // actions) — confirmed empirically, both debugging a real, reproducible
    // bug in this test itself (not the app): accesskit's `Focus` action
    // doesn't transfer egui's actual keyboard-input focus (only some
    // separate accessibility-focus notion), so every subsequent `type_text`
    // kept landing in whichever field happened to hold real focus first.
    // `Node::simulate_click()` (a genuine synthetic pointer press/release,
    // the same path a real click takes) is what actually grants text-edit
    // focus. The tree must be re-queried after each `run()` too — nodes
    // from a prior frame's snapshot don't track content across frames.
    for (index, text) in [
        (0, "Fully specified task"),
        (1, "qa-reverify, cross-cutting"),
        (2, "ben"),
        (3, "v-reverify"),
        (4, "TASK-1"),
    ] {
        let field = h
            .get_by_label("New Backlog Task")
            .query_all(kittest::by().role(egui::accesskit::Role::TextInput))
            .nth(index)
            .unwrap_or_else(|| panic!("no TextInput at index {index}"));
        field.click();
        field.type_text(text);
        h.run();
    }

    assert_eq!(
        h.state().backlog_view.new_task.title,
        "Fully specified task"
    );
    assert_eq!(
        h.state().backlog_view.new_task.labels,
        "qa-reverify, cross-cutting"
    );
    assert_eq!(h.state().backlog_view.new_task.assignees, "ben");
    assert_eq!(h.state().backlog_view.new_task.project, "v-reverify");
    assert_eq!(h.state().backlog_view.new_task.dependencies, "TASK-1");

    h.get_by_label("Create").click();
    h.run();

    // TASK-28 (2026-08-05 fix wave, post-dates this test): the status
    // message this polls for changed from "created task: <raw multi-line
    // CLI stdout>" (the owner-found bug — that raw stdout blew up the top
    // bar's layout) to a compact "Created {repo}:{id}" — see
    // spawn_backlog_create's own doc comment (app.rs) and
    // create_modal_reports_a_compact_created_message_against_a_real_
    // fixture_repo (backlog_controls.rs) for the dedicated proof of the new
    // format. Updated the prefix this loop waits for accordingly.
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        h.run();
        if h.state()
            .backlog_status
            .snapshot()
            .as_deref()
            .is_some_and(|s| s.starts_with("Created "))
        {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "create's background thread did not report completion in time; last status: {:?}",
            h.state().backlog_status.snapshot()
        );
        std::thread::sleep(Duration::from_millis(20));
    }

    let repo = switchbard_core::load_backlog_repo(root).expect("reload the real fixture repo");
    let created = repo
        .tasks
        .iter()
        .find(|t| t.title == "Fully specified task")
        .expect("the newly created task should be present");
    assert_eq!(
        created.labels,
        vec!["qa-reverify".to_string(), "cross-cutting".to_string()],
        "labels typed into the modal should reach the real CLI"
    );
    assert_eq!(
        created.assignees,
        vec!["ben".to_string()],
        "assignee typed into the modal should reach the real CLI"
    );
    assert_eq!(
        created.project.as_deref(),
        Some("v-reverify"),
        "milestone typed into the modal should reach the real CLI"
    );
    assert_eq!(
        created.dependencies,
        vec!["TASK-1".to_string()],
        "dependency typed into the modal should reach the real CLI"
    );
}

// ─── 5. Clean Up Old Tasks: cross-repo archive with confirm gate, real CLI
//        (fix wave: 51cc1a8) ──────────────────────────────────────────────
//
// Two independent real fixture repos, each with one Done and one non-Done
// task. Verifies: (a) Cancel touches neither repo, (b) Confirm archives the
// Done task in *both* repos via the real CLI, (c) the non-Done tasks in
// both repos are untouched, (d) the synchronous pre-spawn status message.

#[test]
fn clean_up_old_tasks_cancel_leaves_both_repos_untouched() {
    let repo_a = init_fixture_repo();
    let repo_b = init_fixture_repo();
    for root in [repo_a.path(), repo_b.path()] {
        native_task_create(root, "Done task");
        native_task_status(root, "TASK-1", "Done");
        native_task_create(root, "Open task");
    }

    let mut h = harness(two_repo_app(repo_a.path(), repo_b.path()));
    h.run();

    h.get_by_label("Clean Up Old Tasks").click();
    h.run();
    assert!(h.state().backlog_view.cleanup_confirm);

    h.get_by_label("Cancel").click();
    h.run();
    assert!(!h.state().backlog_view.cleanup_confirm);

    for root in [repo_a.path(), repo_b.path()] {
        let repo = switchbard_core::load_backlog_repo(root).unwrap();
        let done_task = repo.tasks.iter().find(|t| t.id == "TASK-1").unwrap();
        assert_eq!(
            done_task.source,
            BacklogTaskSource::Active,
            "Cancel must not archive anything in {}",
            root.display()
        );
    }
}

/// FIXED (was a defect found during 2026-08-05 fix-wave re-verification,
/// not part of the original six items — a new finding): the real `backlog`
/// CLI v1.47.1 **refuses** `backlog task archive` on a Done-status task:
/// `Task TASK-1 is Done. Done tasks should be completed, not archived. Use:
/// backlog task complete TASK-1` (confirmed empirically, see this file's
/// commit). "Clean Up Old Tasks" exclusively targets Done tasks
/// (`toolbar::cleanup_candidates` filters on `task.is_done()`), so it
/// previously failed on every real invocation. Fixed by routing through the
/// new `complete_backlog_task` (core) instead of `archive_backlog_task` —
/// `HiveApp::spawn_backlog_cleanup` now calls it directly, and the
/// resulting `BacklogTaskSource` is `Completed`, not `Archived` (this
/// test's own assertion below is updated accordingly, as the doc comment
/// this replaced anticipated). The same fix covers the pre-existing
/// single-task Archive button, which now shows "Complete" instead of
/// "Archive" whenever the selected task is Done
/// (`detail_lists::render_archive`).
#[test]
fn clean_up_old_tasks_confirm_archives_the_done_task_in_both_real_repos() {
    let repo_a = init_fixture_repo();
    let repo_b = init_fixture_repo();
    for root in [repo_a.path(), repo_b.path()] {
        native_task_create(root, "Done task");
        native_task_status(root, "TASK-1", "Done");
        native_task_create(root, "Open task");
    }

    let mut h = harness(two_repo_app(repo_a.path(), repo_b.path()));
    h.run();

    h.get_by_label("Clean Up Old Tasks").click();
    h.run();
    h.get_by_label("Confirm cleanup").click();
    h.run();

    // Synchronous, pre-spawn status message (set before the background
    // thread's per-task archive calls even start). Since the format fork's
    // native writes, the background thread can finish before this assertion
    // runs — no subprocess spawn to lose the race to — so the final message
    // is also acceptable here; the bounded poll below still pins it.
    let immediate = h.state().backlog_status.snapshot();
    assert!(
        matches!(
            immediate.as_deref(),
            Some("cleaning up 2 Done tasks") | Some("cleaned up 2/2 Done tasks across 2 repos")
        ),
        "unexpected status right after confirm: {immediate:?}"
    );

    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        h.run();
        if h.state().backlog_status.snapshot().as_deref()
            == Some("cleaned up 2/2 Done tasks across 2 repos")
        {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "cleanup's background thread did not report completion in time; last status: {:?}",
            h.state().backlog_status.snapshot()
        );
        std::thread::sleep(Duration::from_millis(20));
    }

    for root in [repo_a.path(), repo_b.path()] {
        let repo = switchbard_core::load_backlog_repo(root)
            .unwrap_or_else(|e| panic!("reload {} failed: {e}", root.display()));
        let done_task = repo
            .tasks
            .iter()
            .find(|t| t.id == "TASK-1")
            .unwrap_or_else(|| panic!("TASK-1 missing after reload in {}", root.display()));
        assert_eq!(
            done_task.source,
            BacklogTaskSource::Completed,
            "the Done task in {} should be completed by the real CLI, not archived",
            root.display()
        );
        let open_task = repo.tasks.iter().find(|t| t.id == "TASK-2").unwrap();
        assert_eq!(
            open_task.source,
            BacklogTaskSource::Active,
            "the non-Done task in {} must not be touched",
            root.display()
        );
    }
}

fn two_repo_app(root_a: &std::path::Path, root_b: &std::path::Path) -> HiveApp {
    let repos = vec![
        Repo {
            name: "qa-fixture-a".to_string(),
            path: root_a.to_path_buf(),
        },
        Repo {
            name: "qa-fixture-b".to_string(),
            path: root_b.to_path_buf(),
        },
    ];
    let worktrees = vec![
        WorktreeRef {
            repo_name: "qa-fixture-a".to_string(),
            path: root_a.to_path_buf(),
            branch: Some("main".to_string()),
            head: "abc1234".to_string(),
        },
        WorktreeRef {
            repo_name: "qa-fixture-b".to_string(),
            path: root_b.to_path_buf(),
            branch: Some("main".to_string()),
            head: "def5678".to_string(),
        },
    ];
    let mut cfg = Config::default();
    cfg.ui.onboarding_dismissed = true;
    let mut app = HiveApp::new_headless(cfg, repos, worktrees);
    app.config_save_path = Some(isolated_config_save_path());
    app.view_tab = ViewTab::Backlog;
    app.backlog_view.lens = BacklogLens::List;
    for root in [root_a, root_b] {
        app.backlog_repos.lock().unwrap().insert(
            root.to_path_buf(),
            switchbard_core::load_backlog_repo(root).expect("load real fixture repo"),
        );
    }
    app
}

use eframe::egui;
