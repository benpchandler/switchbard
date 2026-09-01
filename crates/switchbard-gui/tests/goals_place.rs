//! Goals **place** tests (TASK-101): the real Goals index (rows, pace
//! chips, manual check-in pre-fill + submit, "automatic" for measured
//! goals, edit target) and the goal page (this-week/history/Inputs cards,
//! attach/detach), against `Place::Goals`'s real render path
//! (`ui::places::goals::render_goals_place`) — mounted through
//! `HiveApp::render_ui`, same discipline as `nav_ia_v2.rs`.
//!
//! The fixture-repo tests here (`native_backlog_init` + real
//! `switchbard_core` writes, then a bounded poll of `backlog_status`) prove
//! the spawn methods this module added (`spawn_goal_checkin`,
//! `spawn_goal_edit_target`, `spawn_goal_attach_input`,
//! `spawn_goal_detach_input`) reach the real core write layer end to end,
//! not just that a click flips local view state — same discipline as
//! `backlog_controls.rs`'s
//! `save_button_completes_a_real_write_round_trip_against_a_real_fixture_repo`.

mod common;

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use common::{harness, isolated_config_save_path, REPO_NAME, REPO_PATH};
use eframe::egui;
use egui_kittest::kittest::{self, Queryable};
use egui_kittest::{Harness, Node};
use switchbard_core::config::Config;
use switchbard_core::{
    load_backlog_repo, BacklogRepo, BacklogTask, BacklogTaskSource, GoalCheckIn, GoalDef,
    GoalInputs, GoalMeasure, GoalWeek, NewBacklogTask, NewGoal, Repo, RepoRanking, WorktreeRef,
};
use switchbard_gui::app::HiveApp;
use switchbard_gui::runtime::Place;

fn current_week() -> String {
    switchbard_core::week_monday_of(chrono::Local::now().date_naive())
        .format("%Y-%m-%d")
        .to_string()
}

fn backlog_task(id: &str, title: &str, status: &str, project: Option<&str>) -> BacklogTask {
    BacklogTask {
        id: id.to_string(),
        title: title.to_string(),
        status: status.to_string(),
        priority: "medium".to_string(),
        assignees: vec![],
        labels: vec![],
        dependencies: vec![],
        references: vec![],
        project: project.map(|p| p.to_string()),
        parent: None,
        created_date: Some("2026-06-20 12:00".to_string()),
        updated_date: Some(current_week()),
        description: "body".to_string(),
        implementation_plan: String::new(),
        implementation_notes: String::new(),
        final_summary: String::new(),
        acceptance_criteria: vec![],
        definition_of_done: vec![],
        source: BacklogTaskSource::Active,
        path: PathBuf::from(format!("{REPO_PATH}/backlog/tasks/{id}.md")),
    }
}

fn backlog_repo(goals: Vec<GoalDef>, tasks: Vec<BacklogTask>) -> BacklogRepo {
    BacklogRepo {
        root: PathBuf::from(REPO_PATH),
        tasks,
        warnings: vec![],
        project_defs: vec![],
        initiative_defs: vec![],
        goals,
        ranking: RepoRanking::default(),
        loaded_at_unix: 0,
        configured_statuses: vec!["To Do".into(), "In Progress".into(), "Done".into()],
    }
}

/// A manual goal with a target set for the *current* week and an optional
/// seeded check-in. No check-in => actual 0 => always `Behind` (0 * 7 never
/// clears `target * days_elapsed` for a positive target) — deterministic
/// regardless of which weekday the suite runs on.
fn manual_goal(name: &str, target: i64, checkin: Option<i64>) -> GoalDef {
    let week = current_week();
    let checkins = checkin
        .map(|v| {
            vec![GoalCheckIn {
                date: week.clone(),
                value: v,
            }]
        })
        .unwrap_or_default();
    GoalDef {
        name: name.to_string(),
        unit: "users".to_string(),
        measure: GoalMeasure::Manual,
        scope: None,
        inputs: GoalInputs::default(),
        weeks: BTreeMap::from([(week, GoalWeek { target, checkins })]),
    }
}

fn tasks_goal(name: &str, target: i64, scope: Option<&str>, inputs: GoalInputs) -> GoalDef {
    let week = current_week();
    GoalDef {
        name: name.to_string(),
        unit: "tasks".to_string(),
        measure: GoalMeasure::Tasks,
        scope: scope.map(|s| s.to_string()),
        inputs,
        weeks: BTreeMap::from([(
            week,
            GoalWeek {
                target,
                checkins: vec![],
            },
        )]),
    }
}

fn app_with_repo(repo: BacklogRepo) -> HiveApp {
    let repos = vec![Repo {
        name: REPO_NAME.to_string(),
        path: PathBuf::from(REPO_PATH),
    }];
    let worktrees = vec![WorktreeRef {
        repo_name: REPO_NAME.to_string(),
        path: PathBuf::from(REPO_PATH),
        branch: Some("main".to_string()),
        head: "abc1234".to_string(),
    }];
    let mut cfg = Config::default();
    cfg.ui.onboarding_dismissed = true;
    let mut app = HiveApp::new_headless(cfg, repos, worktrees);
    app.config_save_path = Some(isolated_config_save_path());
    app.place = Place::Goals;
    app.backlog_repos
        .lock()
        .unwrap()
        .insert(PathBuf::from(REPO_PATH), repo);
    app
}

/// Native fixture init — same directory shape `backlog_controls.rs`'s own
/// `native_backlog_init` uses (no external CLI post-format-fork, TASK-67).
fn native_backlog_init(root: &std::path::Path) {
    std::fs::create_dir_all(root.join("backlog/tasks")).expect("fixture layout");
    std::fs::write(
        root.join("backlog/config.yml"),
        "statuses: [\"To Do\", \"In Progress\", \"Done\"]\n",
    )
    .expect("fixture config");
}

fn app_for_fixture(root: &std::path::Path) -> HiveApp {
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
    app.place = Place::Goals;
    app.backlog_repos.lock().unwrap().insert(
        root.to_path_buf(),
        load_backlog_repo(root).expect("load real fixture repo"),
    );
    app
}

/// Bounded poll of `backlog_status` for a spawned write's completion
/// message — the same wait shape
/// `save_button_completes_a_real_write_round_trip_against_a_real_fixture_repo`
/// uses, since a background-thread write has no other synchronous signal.
fn wait_for_status(harness: &mut Harness<'static, HiveApp>, expected: &str) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        harness.run_steps(4);
        let status = harness.state().backlog_status.snapshot();
        if status.as_deref() == Some(expected) {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "background write did not report completion in time; expected {expected:?}, last status: {status:?}"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// This module's painted icon buttons (`theme::icon_button_label`) carry
/// their accessible name via *both* an explicit `widget_info` fill (role
/// `Button`) and `.on_hover_text` (which egui also exposes to AccessKit as
/// its own `Label` node, independent of whether anything is actually
/// hovering) — so a bare `by().label(...)` matches two nodes per icon
/// button. Scope to `role(Button)` wherever a test locates one of these by
/// label, matching how `backlog_controls.rs`'s own helpers scope text
/// inputs by role for the identical reason (see its `detail_text_input`).
fn icon_buttons<'a>(harness: &'a Harness<'static, HiveApp>, label: &'a str) -> Vec<Node<'a>> {
    harness
        .query_all(
            kittest::by()
                .label(label)
                .role(egui::accesskit::Role::Button),
        )
        .collect()
}

fn icon_button<'a>(harness: &'a Harness<'static, HiveApp>, label: &'a str) -> Node<'a> {
    harness.get(
        kittest::by()
            .label(label)
            .role(egui::accesskit::Role::Button),
    )
}

// ---------------------------------------------------------------------
// Index
// ---------------------------------------------------------------------

#[test]
fn index_renders_rows_and_pace_chips_from_fixtures() {
    let repo = backlog_repo(
        vec![
            manual_goal("Behind goal", 5, None),
            manual_goal("Met goal", 1, Some(1)),
            tasks_goal("Auto goal", 8, Some("Proj"), GoalInputs::default()),
        ],
        vec![],
    );
    let mut harness = harness(app_with_repo(repo));
    harness.run();

    assert!(
        harness.query_all_by_label("Goals").next().is_some(),
        "index header renders (also matches the nav's own Goals place row label)"
    );
    assert!(harness.query_by_label("Behind goal").is_some());
    assert!(harness.query_by_label("Met goal").is_some());
    assert!(harness.query_by_label("Auto goal").is_some());
    // "Behind goal" and "Auto goal" (0 actual, positive target) are both
    // deterministically Behind — see `manual_goal`'s own doc.
    assert_eq!(
        harness.query_all_by_label("behind").count(),
        2,
        "both zero-actual goals show the Behind pace chip"
    );
    assert!(harness.query_by_label("met").is_some(), "Met pace chip");
    assert!(
        harness.query_all_by_label("automatic").next().is_some(),
        "the measured goal shows 'automatic', not an input"
    );
    assert_eq!(
        icon_buttons(&harness, "Check in").len(),
        2,
        "both manual goals carry the inline check-in affordance; the measured one does not"
    );
    assert_eq!(
        icon_buttons(&harness, "Edit target").len(),
        3,
        "every goal, manual or measured, carries the edit-target pencil"
    );
}

#[test]
fn empty_state_offers_new_goal_and_roll_last_week() {
    let repo = backlog_repo(vec![], vec![]);
    let mut harness = harness(app_with_repo(repo));
    harness.run();

    assert!(harness.query_by_label("No goals this week").is_some());
    assert!(harness.query_by_label("+ New goal").is_some());
    assert!(harness.query_by_label("Roll last week").is_some());

    harness.get_by_label("+ New goal").click();
    harness.run();
    assert!(
        harness.state().backlog_view.new_goal.open,
        "the empty state's + New goal opens the same New Goal modal"
    );
}

#[test]
fn manual_checkin_prefills_the_cumulative_value_and_submits_through_the_real_write_layer() {
    let fixture = tempfile::tempdir().expect("tempdir");
    let root = fixture.path();
    native_backlog_init(root);
    let week = current_week();
    switchbard_core::create_goal(
        root,
        &NewGoal {
            name: "Onboard users".to_string(),
            unit: "users".to_string(),
            measure: GoalMeasure::Manual,
            scope: None,
            week: week.clone(),
            target: 5,
        },
    )
    .expect("create goal");
    switchbard_core::check_in_goal(root, "Onboard users", &week, &week, 2).expect("seed check-in");

    let app = app_for_fixture(root);
    // Reload after the seed check-in above (the fixture write happened
    // before the repo snapshot was taken).
    app.backlog_repos.lock().unwrap().insert(
        root.to_path_buf(),
        load_backlog_repo(root).expect("reload real fixture repo"),
    );
    let mut harness = harness(app);
    harness.run();

    // Q11=B: the check-in draft pre-fills with the CURRENT cumulative
    // value (2), never 0 — the value typed and submitted IS the new total.
    let draft = *harness
        .state()
        .backlog_view
        .goal_checkin_drafts
        .get(&(root.to_path_buf(), "Onboard users".to_string()))
        .expect("the draft should be seeded on first render");
    assert_eq!(draft, 2, "the draft pre-fills with the actual, not 0");

    icon_button(&harness, "Check in").click();
    harness.run_steps(4);
    wait_for_status(&mut harness, "checked in Onboard users: 2");

    let repo = load_backlog_repo(root).expect("reload real fixture repo");
    let goal = repo
        .goals
        .iter()
        .find(|g| g.name == "Onboard users")
        .expect("goal survives");
    let checkins = &goal.weeks[&week].checkins;
    assert_eq!(
        checkins.len(),
        2,
        "a second check-in was appended, not the first replaced"
    );
    assert_eq!(
        checkins.last().unwrap().value,
        2,
        "the submitted value is the pre-filled cumulative total"
    );
}

#[test]
fn edit_target_wires_through_the_real_write_layer() {
    let fixture = tempfile::tempdir().expect("tempdir");
    let root = fixture.path();
    native_backlog_init(root);
    let week = current_week();
    switchbard_core::create_goal(
        root,
        &NewGoal {
            name: "IA decision record".to_string(),
            unit: "docs".to_string(),
            measure: GoalMeasure::Manual,
            scope: None,
            week: week.clone(),
            target: 1,
        },
    )
    .expect("create goal");

    let mut harness = harness(app_for_fixture(root));
    harness.run();

    icon_button(&harness, "Edit target").click();
    harness.run();
    assert!(harness.state().goals_view.edit_target.open);
    assert_eq!(
        harness.state().goals_view.edit_target.target,
        1,
        "the editor opens pre-filled with the current target"
    );

    harness.state_mut().goals_view.edit_target.target = 9;
    harness.run();
    harness.get_by_label("Save").click();
    harness.run_steps(4);
    wait_for_status(&mut harness, "IA decision record: target set to 9");

    let repo = load_backlog_repo(root).expect("reload real fixture repo");
    let goal = repo
        .goals
        .iter()
        .find(|g| g.name == "IA decision record")
        .expect("goal survives");
    assert_eq!(goal.weeks[&week].target, 9);
}

// ---------------------------------------------------------------------
// Goal page
// ---------------------------------------------------------------------

#[test]
fn goal_page_renders_crumb_this_week_history_and_inputs_cards() {
    let week = current_week();
    let goal = GoalDef {
        name: "Close out Stack Ranking".to_string(),
        unit: "tasks".to_string(),
        measure: GoalMeasure::Tasks,
        scope: Some("Stack Ranking".to_string()),
        inputs: GoalInputs {
            tasks: vec!["TASK-61".to_string()],
            projects: vec!["Stack Ranking".to_string()],
        },
        weeks: BTreeMap::from([(
            week,
            GoalWeek {
                target: 4,
                checkins: vec![],
            },
        )]),
    };
    let tasks = vec![
        backlog_task("TASK-61", "Landing worker", "To Do", None),
        backlog_task("TASK-70", "Member task", "Done", Some("Stack Ranking")),
    ];
    let repo = backlog_repo(vec![goal], tasks);
    let mut app = app_with_repo(repo);
    app.goals_view.selected_goal = Some((
        PathBuf::from(REPO_PATH),
        "Close out Stack Ranking".to_string(),
    ));
    let mut harness = harness(app);
    harness.run();

    assert!(
        harness.query_all_by_label("Goals").next().is_some(),
        "the crumb's 'Goals' link back to the index renders (also matches the nav's own Goals place row label)"
    );
    assert!(
        harness
            .query_all_by_label("Close out Stack Ranking")
            .next()
            .is_some(),
        "the goal page header renders the name"
    );
    assert!(
        harness.query_by_label("Roll into next week").is_some(),
        "this-week card renders its roll action"
    );
    assert!(
        harness.query_all_by_label("Edit target").next().is_some(),
        "this-week card carries edit target too"
    );
    assert!(
        harness
            .query_by_label("History · weekly outcomes")
            .is_some(),
        "history card renders"
    );
    assert!(
        harness
            .query_by_label("Inputs · what this goal counts")
            .is_some(),
        "inputs card renders"
    );
    assert!(
        harness
            .query_all_by_label("Stack Ranking · project")
            .next()
            .is_some(),
        "the attached project input row renders"
    );
    assert!(
        harness
            .query_all_by_label("TASK-61 · Landing worker")
            .next()
            .is_some(),
        "the attached task input row renders"
    );
    assert_eq!(
        icon_buttons(&harness, "Detach input").len(),
        2,
        "each attached input carries its own detach icon"
    );
}

#[test]
fn a_goal_with_no_inputs_shows_the_attach_affordance_and_no_rows() {
    let week = current_week();
    let goal = GoalDef {
        name: "No inputs yet".to_string(),
        unit: "tasks".to_string(),
        measure: GoalMeasure::Tasks,
        scope: None,
        inputs: GoalInputs::default(),
        weeks: BTreeMap::from([(
            week,
            GoalWeek {
                target: 2,
                checkins: vec![],
            },
        )]),
    };
    let repo = backlog_repo(vec![goal], vec![]);
    let mut app = app_with_repo(repo);
    app.goals_view.selected_goal = Some((PathBuf::from(REPO_PATH), "No inputs yet".to_string()));
    let mut harness = harness(app);
    harness.run();

    assert!(harness.query_by_label("Nothing attached yet.").is_some());
    assert!(harness.query_by_label("+ Attach task or project").is_some());
    assert!(
        harness.query_by_label("Detach input").is_none(),
        "no inputs, no detach icons"
    );
}

#[test]
fn inputs_card_attach_and_detach_wire_through_the_real_write_layer() {
    let fixture = tempfile::tempdir().expect("tempdir");
    let root = fixture.path();
    native_backlog_init(root);
    let week = current_week();
    switchbard_core::create_goal(
        root,
        &NewGoal {
            name: "Dispatch throughput".to_string(),
            unit: "tasks".to_string(),
            measure: GoalMeasure::Tasks,
            scope: None,
            week: week.clone(),
            target: 3,
        },
    )
    .expect("create goal");
    let task_id = switchbard_core::create_backlog_task(
        root,
        &NewBacklogTask {
            title: "Fixture task".to_string(),
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
    .expect("create task");

    let mut app = app_for_fixture(root);
    app.goals_view.selected_goal = Some((root.to_path_buf(), "Dispatch throughput".to_string()));
    let mut harness = harness(app);
    harness.run();

    harness.get_by_label("+ Attach task or project").click();
    harness.run();
    assert!(harness.state().goals_view.attach_input.open);

    // Only the fixture task exists (no project on any task), so exactly one
    // "Attach" row button renders.
    harness.get_by_label("Attach").click();
    harness.run_steps(4);
    wait_for_status(&mut harness, "attached 1 input(s) to Dispatch throughput");

    let repo = load_backlog_repo(root).expect("reload real fixture repo");
    let goal = repo
        .goals
        .iter()
        .find(|g| g.name == "Dispatch throughput")
        .expect("goal survives");
    assert!(
        goal.inputs
            .tasks
            .iter()
            .any(|t| t.eq_ignore_ascii_case(&task_id)),
        "the attach wrote the real inputs block"
    );

    harness.state_mut().goals_view.attach_input.open = false;
    harness.run();

    icon_button(&harness, "Detach input").click();
    harness.run_steps(4);
    wait_for_status(&mut harness, "detached 1 input(s) from Dispatch throughput");

    let repo = load_backlog_repo(root).expect("reload real fixture repo");
    let goal = repo
        .goals
        .iter()
        .find(|g| g.name == "Dispatch throughput")
        .expect("goal survives");
    assert!(goal.inputs.is_empty(), "the detach removed the input block");
}
