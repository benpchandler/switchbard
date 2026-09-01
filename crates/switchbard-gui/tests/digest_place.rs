//! TASK-99 — the Digest place: goal cards leading, the "In flight" section,
//! and the "Needs a human" attention feed. Mounts the real window via
//! `common::harness`, same discipline as `tests/nav_ia_v2.rs`: these prove
//! the actual render path, not an isolated fragment.
//!
//! Coverage matches the mission brief's evidence list: goal cards lead +
//! zero-goal empty state; in-flight rows; feed rows built from fixtures
//! (failed run -> Retry, unattributed listener -> port row, removable
//! worktree -> worktree row); deep-link navigation; and — the point of
//! "reuse the owning surface's verb, don't fork it" — that the confirm
//! state a Digest kill arms is the *same* `HiveApp` field the owning
//! surface's own kill button arms.

mod common;

use std::path::PathBuf;

use common::{harness, isolated_config_save_path, REPO_PATH};
use egui_kittest::kittest::{NodeT, Queryable};
use switchbard_core::config::Config;
use switchbard_core::dispatch_inspect::{now_unix, DispatchRun, DispatchRunLiveness};
use switchbard_core::{
    AttributedListener, BacklogChecklistItem, BacklogRepo, BacklogTask, BacklogTaskSource,
    DispatchOptions, Fact, GoalDef, GoalInputs, GoalMeasure, GoalWeek, LandedEvidence,
    LocalListener, Repo, RepoRanking, WorktreeRef, WorktreeStaleness, DISPATCHING_LABEL,
    DISPATCH_FAILED_LABEL,
};
use switchbard_gui::app::HiveApp;
use switchbard_gui::runtime::{Place, TasksView, WorktreeMeta};

fn task(id: &str, title: &str, status: &str, labels: &[&str], notes: &str) -> BacklogTask {
    BacklogTask {
        id: id.to_string(),
        title: title.to_string(),
        status: status.to_string(),
        priority: "medium".to_string(),
        assignees: vec![],
        labels: labels.iter().map(|l| l.to_string()).collect(),
        dependencies: vec![],
        references: vec![],
        project: None,
        parent: None,
        created_date: Some("2026-06-20 12:00".to_string()),
        updated_date: Some("2026-06-20 12:00".to_string()),
        description: "body".to_string(),
        implementation_plan: String::new(),
        implementation_notes: notes.to_string(),
        final_summary: String::new(),
        acceptance_criteria: vec![BacklogChecklistItem {
            index: 1,
            checked: false,
            text: "criterion".to_string(),
        }],
        definition_of_done: vec![],
        source: BacklogTaskSource::Active,
        path: PathBuf::from(format!("{REPO_PATH}/backlog/tasks/{id}.md")),
    }
}

fn goal_def(name: &str) -> GoalDef {
    let week = switchbard_core::week_monday_of(chrono::Local::now().date_naive())
        .format("%Y-%m-%d")
        .to_string();
    GoalDef {
        name: name.to_string(),
        unit: "tasks".to_string(),
        measure: GoalMeasure::Manual,
        scope: None,
        inputs: GoalInputs::default(),
        weeks: std::collections::BTreeMap::from([(
            week,
            GoalWeek {
                target: 4,
                checkins: vec![],
            },
        )]),
    }
}

/// Regression fixture: an earlier version of `render_goal_cards_for_digest_
/// place` laid cards out in a `horizontal_wrapped` row to match the mock's
/// side-by-side `goalrow` — but `render_goal_card`'s frame claims its whole
/// row's width to pin the favorite star flush right (every other caller
/// stacks vertically), so the second card rendered invisibly on top of the
/// first. The fix (`render_compact_goal_grid`/`render_compact_goal_card`)
/// gives each card an explicit `allocate_ui` width instead of sharing the
/// row's; `goal_cards_lead_the_digest_place_ahead_of_in_flight_and_the_feed`
/// now asserts the two cards' painted rects directly (not just that both are
/// present in the accessibility tree, which the old bug's overlap would
/// still have satisfied) so a future regression fails a kittest assertion,
/// not just a pixel diff a human has to notice.
fn second_goal_def(name: &str) -> GoalDef {
    let week = switchbard_core::week_monday_of(chrono::Local::now().date_naive())
        .format("%Y-%m-%d")
        .to_string();
    GoalDef {
        name: name.to_string(),
        unit: "tasks".to_string(),
        measure: GoalMeasure::Manual,
        scope: None,
        inputs: GoalInputs::default(),
        weeks: std::collections::BTreeMap::from([(
            week,
            GoalWeek {
                target: 8,
                checkins: vec![],
            },
        )]),
    }
}

/// A headless `HiveApp` with one tracked repo/worktree, `tasks` seeded into
/// its backlog cache, and `runs` seeded into the dispatch-run cache the way
/// `workers::refresh_dispatch_runs` would have left them. Lands on
/// `Place::Digest` — `HiveApp::new_headless`'s own default — since that is
/// this whole file's subject.
fn app_with(tasks: Vec<BacklogTask>, goals: Vec<GoalDef>, runs: Vec<DispatchRun>) -> HiveApp {
    let repos = vec![Repo {
        name: "demo".to_string(),
        path: PathBuf::from(REPO_PATH),
    }];
    let worktrees = vec![WorktreeRef {
        repo_name: "demo".to_string(),
        path: PathBuf::from(REPO_PATH),
        branch: Some("main".to_string()),
        head: "abc1234".to_string(),
    }];
    let mut cfg = Config::default();
    cfg.ui.onboarding_dismissed = true;
    let mut app = HiveApp::new_headless(cfg, repos, worktrees);
    app.config_save_path = Some(isolated_config_save_path());
    app.backlog_repos.lock().unwrap().insert(
        PathBuf::from(REPO_PATH),
        BacklogRepo {
            root: PathBuf::from(REPO_PATH),
            tasks,
            warnings: vec![],
            project_defs: vec![],
            initiative_defs: vec![],
            goals,
            ranking: RepoRanking::default(),
            loaded_at_unix: 0,
            configured_statuses: vec![
                "To Do".into(),
                "In Progress".into(),
                "In Review".into(),
                "Done".into(),
            ],
        },
    );
    let mut cached = app.dispatch_runs.lock().unwrap();
    for run in runs {
        cached.insert((PathBuf::from(REPO_PATH), run.task_id.clone()), run);
    }
    drop(cached);
    app
}

fn run_with(task_id: &str, age_secs: u64, liveness: DispatchRunLiveness) -> DispatchRun {
    DispatchRun {
        task_id: task_id.to_string(),
        branch: format!("dispatch/{}", task_id.to_lowercase()),
        worktree_path: PathBuf::from(format!("{REPO_PATH}/.worktrees/dispatch-{task_id}")),
        worktree_exists: false,
        log_path: Some(PathBuf::from("/tmp/switchbard-logs/dispatch.log")),
        prompt_path: None,
        started_at_unix: Some(now_unix().saturating_sub(age_secs)),
        log_bytes: 0,
        log_modified_unix: None,
        liveness,
        progress: switchbard_core::dispatch_inspect::RunProgress::default(),
    }
}

/// A worktree meta that `removal_safety` evaluates to `RemovalVerdict::Safe`
/// — the same shape `runtime::is_retired_worktree` (and therefore the
/// top-bar "N retired worktrees" nudge) requires: no lock, clean, and
/// merged into its trunk.
fn removable_meta() -> WorktreeMeta {
    WorktreeMeta {
        dirty_files: Some(vec![]),
        lock: Fact::Known(None),
        staleness: Some(WorktreeStaleness::Merged {
            base: "main".to_string(),
            evidence: LandedEvidence::Ancestry,
        }),
        ..Default::default()
    }
}

// ─── goal cards lead ────────────────────────────────────────────────────

#[test]
fn goal_cards_lead_the_digest_place_ahead_of_in_flight_and_the_feed() {
    let mut app = app_with(
        vec![task("TASK-1", "Some task", "To Do", &[], "")],
        vec![
            goal_def("Close out Stack Ranking"),
            second_goal_def("Dispatch throughput"),
        ],
        vec![],
    );
    app.place = Place::Digest;
    let mut harness = harness(app);
    harness.run();

    assert!(
        harness.query_by_label("Close out Stack Ranking").is_some(),
        "the goal card should render"
    );
    assert!(
        harness.query_by_label("Dispatch throughput").is_some(),
        "a second goal card must render too, not be hidden behind the first \
         (regression: see second_goal_def's own doc)"
    );
    // The two cards' own labels must have distinct, non-overlapping painted
    // rects — not just both be present in the accessibility tree, which the
    // old horizontal_wrapped bug's overlap would still have satisfied (see
    // second_goal_def's doc).
    let rect_of = |label: &str| {
        harness
            .query_by_label(label)
            .and_then(|node| node.accesskit_node().bounding_box())
            .unwrap_or_else(|| panic!("no painted rect for '{label}'"))
    };
    let first = rect_of("Close out Stack Ranking");
    let second = rect_of("Dispatch throughput");
    assert_ne!(
        (first.x0, first.y0),
        (second.x0, second.y0),
        "the two goal cards must not paint at the same position"
    );
    let overlaps = first.x0 < second.x1
        && second.x0 < first.x1
        && first.y0 < second.y1
        && second.y0 < first.y1;
    assert!(
        !overlaps,
        "goal cards overlap: {first:?} vs {second:?} (the render_goal_card \
         width-contract regression this fixture exists to catch)"
    );
    assert!(
        harness.query_by_label("In flight").is_some(),
        "In flight should still render below the goal cards"
    );
    assert!(
        harness.query_by_label("Needs a human").is_some(),
        "the attention feed should still render last"
    );
    let week_chip = harness
        .query_all(egui_kittest::kittest::by())
        .flat_map(|n| [n.accesskit_node().label(), n.value()])
        .flatten()
        .find(|text| text.starts_with("Week of"));
    assert!(
        week_chip.is_some(),
        "the header should carry the week chip (mock §1's htitle row)"
    );
}

/// Owner-reported: with the Tasks place's single-repo picker parked on a
/// repo with no goals, Digest rendered "No goals this week" while another
/// scoped repo had three current-week goals. Digest is a places surface — it
/// aggregates over the sidebar's multi-select scope and must never consult
/// the Tasks picker, which is invisible from here (the place's other
/// sections already follow that rule — see `collect_task_rows`).
#[test]
fn goal_cards_ignore_the_tasks_places_repo_picker() {
    let mut app = app_with(vec![], vec![goal_def("Close out Stack Ranking")], vec![]);
    app.place = Place::Digest;
    app.backlog_view.selected_repo = Some(PathBuf::from("/some/other/repo"));
    let mut harness = harness(app);
    harness.run();

    assert!(
        harness.query_by_label("Close out Stack Ranking").is_some(),
        "the goal card must render even though the Tasks picker points elsewhere"
    );
    assert!(
        harness.query_by_label("No goals this week").is_none(),
        "the zero-goal state must not appear while a scoped repo has goals"
    );
}

/// The zero-goal card's week line must tell the truth about the clock: the
/// mock's literal "ends today." copy was only correct on a Sunday
/// (owner-reported wrong on a Tuesday). It now mirrors the header chip's
/// "day N of 7" formula.
#[test]
fn zero_goal_state_names_the_week_day_not_ends_today() {
    let mut app = app_with(vec![], vec![], vec![]);
    app.place = Place::Digest;
    let mut harness = harness(app);
    harness.run();

    assert!(
        text_containing(&harness, "ends today").is_empty(),
        "no surface may claim the week ends today unless it actually does"
    );
    let today = chrono::Local::now().date_naive();
    let day = (today - switchbard_core::week_monday_of(today)).num_days() + 1;
    assert!(
        !text_containing(&harness, &format!("day {day} of 7")).is_empty(),
        "the zero-goal card carries the real week-clock position"
    );
}

#[test]
fn zero_goal_state_offers_new_goal_and_roll_last_week() {
    let mut app = app_with(vec![], vec![], vec![]);
    app.place = Place::Digest;
    let mut harness = harness(app);
    harness.run();

    assert!(harness.query_by_label("No goals this week").is_some());
    assert!(
        harness.query_by_label("+ New goal").is_some(),
        "mock §7a's doorway into the goal composer"
    );
    assert!(
        harness.query_by_label("Roll last week").is_some(),
        "mock §7a's doorway into HiveApp::spawn_goal_roll"
    );

    assert!(!harness.state().backlog_view.new_goal.open);
    harness.get_by_label("+ New goal").click();
    harness.run();
    assert!(
        harness.state().backlog_view.new_goal.open,
        "+ New goal opens the same composer the Goals place's own doorway does"
    );
}

// ─── in flight ──────────────────────────────────────────────────────────

#[test]
fn in_flight_lists_dispatching_and_in_progress_tasks_and_deep_links_to_tasks() {
    let mut app = app_with(
        vec![
            task(
                "TASK-1",
                "Dispatching task",
                "To Do",
                &[DISPATCHING_LABEL],
                "",
            ),
            task("TASK-2", "In-progress task", "In Progress", &[], ""),
            task("TASK-3", "Done already", "Done", &[], ""),
        ],
        vec![],
        vec![],
    );
    app.place = Place::Digest;
    let mut harness = harness(app);
    harness.run();

    assert!(harness.query_by_label("Dispatching task").is_some());
    assert!(harness.query_by_label("In-progress task").is_some());
    assert!(
        harness.query_by_label("Done already").is_none(),
        "done tasks must not show as in flight"
    );
    assert!(
        harness.query_by_label("DISPATCHING").is_some(),
        "the dispatching row reuses the Tasks place's own dispatch pill"
    );
    assert!(harness.query_by_label("in progress").is_some());

    assert_eq!(harness.state().place, Place::Digest);
    harness.get_by_label("In-progress task").click();
    harness.run();

    assert_eq!(
        harness.state().place,
        Place::Tasks,
        "clicking an in-flight row deep-links to Tasks"
    );
    assert_eq!(harness.state().tasks_view, TasksView::All);
    assert_eq!(
        harness.state().backlog_view.selected_task,
        Some((PathBuf::from(REPO_PATH), "TASK-2".to_string())),
        "the deep link selects the exact task that was clicked"
    );
}

#[test]
fn in_flight_is_scoped_to_the_current_repo_scope() {
    let repo_b = "/tmp/switchbard-ui-test/digest-second";
    let mut app = app_with(
        vec![task("TASK-1", "First repo task", "In Progress", &[], "")],
        vec![],
        vec![],
    );
    app.repos.lock().unwrap().push(Repo {
        name: "second".to_string(),
        path: PathBuf::from(repo_b),
    });
    app.worktrees.lock().unwrap().push(WorktreeRef {
        repo_name: "second".to_string(),
        path: PathBuf::from(repo_b),
        branch: Some("main".to_string()),
        head: "def5678".to_string(),
    });
    app.backlog_repos.lock().unwrap().insert(
        PathBuf::from(repo_b),
        BacklogRepo {
            root: PathBuf::from(repo_b),
            tasks: vec![task("TASK-2", "Second repo task", "In Progress", &[], "")],
            warnings: vec![],
            project_defs: vec![],
            initiative_defs: vec![],
            goals: vec![],
            ranking: RepoRanking::default(),
            loaded_at_unix: 0,
            configured_statuses: vec!["To Do".into(), "In Progress".into(), "Done".into()],
        },
    );
    app.place = Place::Digest;
    app.repo_scope = std::iter::once(PathBuf::from(REPO_PATH)).collect();
    let mut harness = harness(app);
    harness.run();

    assert!(harness.query_by_label("First repo task").is_some());
    assert!(
        harness.query_by_label("Second repo task").is_none(),
        "narrowing scope must hide the other repo's in-flight task"
    );
}

// ─── needs a human: run rows ────────────────────────────────────────────

#[test]
fn failed_run_feed_row_offers_retry_and_deep_links_to_dispatches() {
    let mut app = app_with(
        vec![task(
            "TASK-1",
            "Retry me",
            "To Do",
            &[DISPATCH_FAILED_LABEL],
            "Dispatch failed: agent error after 22m",
        )],
        vec![],
        vec![],
    );
    app.place = Place::Digest;
    let mut harness = harness(app);
    harness.run();

    assert!(
        !text_containing(&harness, "Dispatch failed: TASK-1").is_empty(),
        "the failed-run row should surface the recorded reason"
    );
    assert!(harness.query_by_label("Retry").is_some());
    assert!(
        harness.query_by_label("Kill").is_none(),
        "a failed (not stalled) run offers no Kill — nothing is running to kill"
    );

    // Deep-link: clicking the row's text navigates to Tasks/Dispatches.
    harness
        .get_by_label(
            text_containing(&harness, "Dispatch failed: TASK-1")
                .first()
                .expect("row text present")
                .as_str(),
        )
        .click();
    harness.run();
    assert_eq!(harness.state().place, Place::Tasks);
    assert_eq!(harness.state().tasks_view, TasksView::Dispatches);
}

/// Proves Retry is not a second implementation: it goes through the same
/// `switchbard_core::set_backlog_label` write layer the Tasks place's own
/// Dispatch button uses, against a real fixture repo on disk.
#[test]
fn retry_re_flags_the_task_through_the_real_write_layer() {
    let fixture = tempfile::tempdir().expect("create temp dir");
    let root = fixture.path();
    std::fs::create_dir_all(root.join("backlog/tasks")).expect("fixture layout");
    std::fs::write(
        root.join("backlog/config.yml"),
        "statuses: [\"To Do\", \"In Progress\", \"Done\"]\n",
    )
    .expect("fixture config");
    let id = switchbard_core::create_backlog_task(
        root,
        &switchbard_core::NewBacklogTask {
            title: "Retry fixture".to_string(),
            description: String::new(),
            status: String::new(),
            priority: String::new(),
            acceptance_criteria: vec![],
            parent: None,
            labels: vec![DISPATCH_FAILED_LABEL.to_string()],
            assignees: vec![],
            project: None,
            dependencies: vec![],
        },
    )
    .expect("create fixture task");

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
    app.place = Place::Digest;
    app.backlog_repos.lock().unwrap().insert(
        root.to_path_buf(),
        switchbard_core::load_backlog_repo(root).expect("load the real fixture repo"),
    );

    let mut harness = harness(app);
    harness.run();
    harness.get_by_label("Retry").click();
    harness.run_steps(4);

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        harness.run_steps(4);
        let status = harness.state().backlog_status.snapshot();
        if status.as_deref() == Some(format!("flagged {id} for dispatch").as_str()) {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "Retry's background thread did not report completion in time; last status: {status:?}"
        );
        std::thread::sleep(std::time::Duration::from_millis(20));
    }

    let repo = switchbard_core::load_backlog_repo(root).expect("reload the real fixture repo");
    let reflagged = repo.tasks.iter().find(|t| t.id == id).expect("task exists");
    assert!(
        reflagged.labels.iter().any(|l| l == switchbard_core::DISPATCH_LABEL),
        "Retry should flag the real task for dispatch, same as the Tasks place's own Dispatch button"
    );
}

#[test]
fn stalled_run_feed_row_kill_arms_the_shared_dispatch_kill_confirm() {
    let past_threshold = DispatchOptions::default().stale_after.as_secs() + 120;
    let mut app = app_with(
        vec![task(
            "TASK-1",
            "Stalled run",
            "To Do",
            &[DISPATCHING_LABEL],
            "",
        )],
        vec![],
        vec![run_with(
            "TASK-1",
            past_threshold,
            DispatchRunLiveness::Alive {
                pgid: 4242,
                supervised: true,
            },
        )],
    );
    app.place = Place::Digest;
    let mut harness = harness(app);
    harness.run();

    assert!(
        harness.query_by_label("Kill").is_some(),
        "a stalled but verified-alive run offers Kill"
    );
    assert!(
        harness.state().dispatch_kill_confirm.is_none(),
        "precondition: nothing armed yet"
    );

    harness.get_by_label("Kill").click();
    harness.run();

    assert_eq!(
        harness.state().dispatch_kill_confirm,
        Some((PathBuf::from(REPO_PATH), "TASK-1".to_string())),
        "Digest arms the exact same HiveApp::dispatch_kill_confirm field the \
         Dispatches view's own Kill button arms — one confirm state, not two"
    );
}

// ─── needs a human: port rows ───────────────────────────────────────────

fn unattributed_listener(port: u16, pgid: i32, pid: u32) -> AttributedListener {
    AttributedListener {
        listener: LocalListener {
            pid,
            pgid,
            port,
            command_name: "rogue-proc".to_string(),
            cwd: None,
        },
        repo_name: None,
        worktree_path: None,
        worktree_branch: None,
    }
}

#[test]
fn unattributed_listener_feed_row_offers_confirm_armed_kill_and_deep_links_to_ops() {
    let mut app = app_with(vec![], vec![], vec![]);
    app.place = Place::Digest;
    app.state
        .lock()
        .unwrap()
        .listeners
        .push(unattributed_listener(5173, 9001, 42));
    let mut harness = harness(app);
    harness.run();

    assert!(
        !text_containing(&harness, ":5173 squatter").is_empty(),
        "the port row should name the port and call it a squatter"
    );
    assert!(harness.query_by_label("Kill").is_some());
    assert!(harness.state().digest_view.port_kill_confirm.is_none());

    harness.get_by_label("Kill").click();
    harness.run();
    assert_eq!(
        harness.state().digest_view.port_kill_confirm,
        Some(9001),
        "Kill arms Digest's own confirm state before signalling anything"
    );
    assert!(harness.query_by_label("Confirm").is_some());

    harness.get_by_label("Cancel").click();
    harness.run();
    assert!(harness.state().digest_view.port_kill_confirm.is_none());

    // Deep-link: clicking the row's text navigates to Ops.
    let row_text = text_containing(&harness, ":5173 squatter")
        .first()
        .expect("row text present")
        .clone();
    assert_eq!(harness.state().place, Place::Digest);
    harness.get_by_label(row_text.as_str()).click();
    harness.run();
    assert_eq!(harness.state().place, Place::Ops);
}

// ─── needs a human: worktree rows ───────────────────────────────────────

#[test]
fn removable_worktree_feed_row_remove_opens_the_shared_confirm_dialog_in_ops() {
    // A real git repo + a real *linked* worktree: `open_remove_worktree_
    // confirm` (the exact verb this test proves Digest reuses) probes real
    // git state (`collect_dirty_files`, `assess_branch_delete`) synchronously
    // at click time, so a fixture path that doesn't exist on disk fails that
    // probe and never arms the dialog at all — this has to be the same kind
    // of fixture `switchbard-core`'s own `worktree_remove` tests use, not a
    // bare `WorktreeRef`.
    let tmp = tempfile::tempdir().expect("create temp dir");
    let repo_root = tmp.path().join("repo");
    std::fs::create_dir(&repo_root).expect("repo dir");
    git(&repo_root, &["init", "-q", "-b", "main"]);
    git(&repo_root, &["config", "user.email", "test@example.com"]);
    git(&repo_root, &["config", "user.name", "Test"]);
    std::fs::write(repo_root.join("README.md"), "hello\n").expect("seed file");
    git(&repo_root, &["add", "."]);
    git(&repo_root, &["commit", "-qm", "init"]);
    let linked = tmp.path().join("wt-retired");
    git(
        &repo_root,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "feat/retired",
            linked.to_str().unwrap(),
        ],
    );

    let mut app = app_with(vec![], vec![], vec![]);
    app.place = Place::Digest;
    app.repos.lock().unwrap()[0].path = repo_root.clone();
    app.worktrees.lock().unwrap()[0].path = repo_root.clone();
    app.worktrees.lock().unwrap().push(WorktreeRef {
        repo_name: "demo".to_string(),
        path: linked.clone(),
        branch: Some("feat/retired".to_string()),
        head: "abc9999".to_string(),
    });
    // `is_retired_worktree`'s cached-fact classification (what the Digest
    // feed itself reads) is independent of the real git probe above — both
    // have to agree this worktree qualifies for the row to exist at all.
    app.meta
        .lock()
        .unwrap()
        .insert(linked.clone(), removable_meta());
    let mut harness = harness(app);
    harness.run();

    assert!(
        !text_containing(&harness, "worktree · removable").is_empty(),
        "a merged, clean, unattached worktree should surface as removable"
    );
    assert!(harness.query_by_label("Remove worktree").is_some());
    assert!(harness
        .state()
        .confirm_remove_worktree
        .lock()
        .unwrap()
        .is_none());

    harness.get_by_label("Remove worktree").click();
    harness.run();

    assert_eq!(
        harness.state().place,
        Place::Ops,
        "Remove worktree deep-links to Ops, where the confirm dialog actually renders"
    );
    assert!(
        harness
            .state()
            .confirm_remove_worktree
            .lock()
            .unwrap()
            .is_some(),
        "Remove worktree opens the exact same HiveApp::open_remove_worktree_confirm \
         state Ops's own row uses — no second confirm dialog"
    );
}

#[test]
fn no_feed_rows_reads_as_nothing_needs_a_human() {
    let mut app = app_with(vec![], vec![], vec![]);
    app.place = Place::Digest;
    let mut harness = harness(app);
    harness.run();

    assert!(harness
        .query_by_label("Nothing needs a human right now.")
        .is_some());
}

/// Minimal real-git fixture setup, mirroring `switchbard-core`'s own
/// `worktree_remove` test module's `run` helper.
fn git(cwd: &std::path::Path, args: &[&str]) {
    let status = std::process::Command::new("git")
        .arg("-C")
        .arg(cwd)
        .args(args)
        .status()
        .expect("git");
    assert!(status.success(), "git {args:?} failed in {cwd:?}");
}

/// Every piece of text in the tree containing `needle` — for assertions
/// about copy that embeds a runtime-assigned id or a live clock. Mirrors
/// `tests/dispatch_operability.rs`'s helper of the same name/shape.
fn text_containing(harness: &egui_kittest::Harness<'_, HiveApp>, needle: &str) -> Vec<String> {
    let mut found: Vec<String> = harness
        .query_all(egui_kittest::kittest::by())
        .flat_map(|node| [node.accesskit_node().label(), node.value()])
        .flatten()
        .filter(|text| text.contains(needle))
        .collect();
    found.sort();
    found.dedup();
    found
}
