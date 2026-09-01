//! QA parity audit (2026-08-05) — pixel screenshots of every Backlog lens,
//! the Create modal, the global search overlay, the saved-views bar, and
//! every dispatch state, in both themes, for `docs/qa/screenshots/`.
//!
//! Same machinery as `ui_snapshot.rs` (`wgpu`, real GPU adapter, `#[ignore]`d
//! for the same reason: pixel output is sensitive to GPU/driver/font
//! differences across machines, so this is not part of the CI gate). Unlike
//! `ui_snapshot.rs`, this file isn't a regression baseline — it exists to
//! produce human-reviewable evidence images, so it writes straight to
//! `docs/qa/screenshots/` (via `SnapshotOptions::output_path`) instead of
//! `tests/snapshots/`, and doesn't assert on the diff (a fresh run always
//! "fails" the diff check against whatever was there before, even though
//! `UPDATE_SNAPSHOTS=1` already wrote the new file — see `snapshot.rs`'s own
//! `maybe_update_snapshot`, which writes unconditionally under that env var
//! and returns `Err` regardless to flag the *comparison* result).
//!
//! Regenerate with:
//! ```sh
//! UPDATE_SNAPSHOTS=1 cargo test -p switchbard-gui --test qa_screenshots -- --ignored
//! ```

mod common;

use std::path::{Path, PathBuf};

use common::{harness, seeded_app, REPO_NAME, REPO_PATH};
use eframe::egui;
use egui_kittest::kittest::{self, Queryable};
use egui_kittest::{Harness, SnapshotOptions};
use switchbard_core::config::ThemeChoice;
use switchbard_core::{
    AttributedListener, BacklogChecklistItem, BacklogRepo, BacklogTask, BacklogTaskSource, Fact,
    GoalDef, GoalInputs, GoalMeasure, GoalWeek, LandedEvidence, LocalListener, WorktreeRef,
    WorktreeStaleness, DISPATCHED_LABEL, DISPATCHING_LABEL, DISPATCH_FAILED_LABEL, DISPATCH_LABEL,
};
use switchbard_gui::app::HiveApp;
use switchbard_gui::runtime::{BacklogLens, Place, WorktreeMeta};

fn output_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/qa/screenshots")
}

fn snapshot(harness: &mut Harness<'_, HiveApp>, name: &str) {
    let options = SnapshotOptions::new().output_path(output_dir());
    // Intentionally ignore the Result — see the module doc. The `.png` this
    // writes (not `.new.png`/`.diff.png`, both gitignored-equivalent scratch
    // files this repo doesn't track) is the artifact we want.
    let _ = harness.try_snapshot_options(name, &options);
}

fn sample_task(id: &str, title: &str, status: &str) -> BacklogTask {
    BacklogTask {
        id: id.to_string(),
        title: title.to_string(),
        status: status.to_string(),
        priority: "high".to_string(),
        assignees: vec!["ben".to_string()],
        labels: vec!["demo".to_string()],
        dependencies: vec![],
        references: vec!["https://example.com/spec".to_string()],
        project: Some("v1".to_string()),
        parent: None,
        created_date: Some("2026-06-01 09:00".to_string()),
        updated_date: Some("2026-06-20 12:00".to_string()),
        description: "## Why\n\nExercises **CommonMark** rendering.".to_string(),
        implementation_plan: "Step one, then step two.".to_string(),
        implementation_notes: "Existing note text.".to_string(),
        final_summary: String::new(),
        acceptance_criteria: vec![BacklogChecklistItem {
            index: 1,
            checked: false,
            text: "Criterion renders".to_string(),
        }],
        definition_of_done: vec![BacklogChecklistItem {
            index: 1,
            checked: true,
            text: "DoD item renders".to_string(),
        }],
        source: BacklogTaskSource::Active,
        path: PathBuf::from(format!(
            "{REPO_PATH}/backlog/tasks/{}.md",
            id.to_lowercase()
        )),
    }
}

fn project_with(tasks: Vec<BacklogTask>) -> BacklogRepo {
    BacklogRepo {
        root: PathBuf::from(REPO_PATH),
        tasks,
        warnings: vec![],
        project_defs: vec![],
        initiative_defs: vec![],
        goals: vec![],
        ranking: switchbard_core::RepoRanking::default(),
        loaded_at_unix: 0,
        configured_statuses: vec![
            "Icebox".into(),
            "To Do".into(),
            "In Progress".into(),
            "In Review".into(),
            "Done".into(),
        ],
    }
}

fn app_with(theme: ThemeChoice, lens: BacklogLens, tasks: Vec<BacklogTask>) -> HiveApp {
    let mut app = seeded_app();
    app.config.ui.theme = theme;
    app.place = Place::Tasks;
    app.backlog_view.lens = lens;
    app.backlog_view.selected_repo = Some(PathBuf::from(REPO_PATH));
    app.backlog_repos
        .lock()
        .unwrap()
        .insert(PathBuf::from(REPO_PATH), project_with(tasks));
    app
}

/// TASK-99: a `HiveApp` landed on `Place::Digest` (the sidebar's own
/// landing place — `app_with`, above, is for the Tasks place's Backlog
/// lenses, a different surface). Seeds an empty-but-tracked backlog repo
/// (no tasks, no goals) so the place renders its real zero-goal empty state
/// (mock §7a) rather than the *different* "no tracked worktrees have a
/// backlog/" state `seeded_app()` alone would hit — callers that want a
/// populated Digest overwrite this same repo entry with real content.
fn digest_place_app(theme: ThemeChoice) -> HiveApp {
    let mut app = seeded_app();
    app.config.ui.theme = theme;
    app.place = Place::Digest;
    app.backlog_repos
        .lock()
        .unwrap()
        .insert(PathBuf::from(REPO_PATH), project_with(vec![]));
    app
}

/// `sample_task`/`task_with` split rather than one giant constructor:
/// dispatch labels/notes are the one axis this file's screenshots vary
/// task-by-task, and `BacklogTask` has no builder methods of its own.
fn task_with(task: BacklogTask, labels: &[&str], notes: &str) -> BacklogTask {
    BacklogTask {
        labels: labels.iter().map(|l| l.to_string()).collect(),
        implementation_notes: notes.to_string(),
        ..task
    }
}

/// A manual-measure weekly goal with one check-in — `actual`/`target` drive
/// the pace pill and meter fill directly, matching mock §1's "behind"
/// (Close out Stack Ranking, 1/4) and "on-track" (Dispatch throughput, 7/8)
/// cards.
fn goal_def(name: &str, target: i64, actual: i64) -> GoalDef {
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
            week.clone(),
            GoalWeek {
                target,
                checkins: vec![switchbard_core::GoalCheckIn {
                    date: week,
                    value: actual,
                }],
            },
        )]),
    }
}

fn shots_for_theme(theme: ThemeChoice) {
    let suffix = format!("_{theme:?}").to_lowercase();

    // Servers view — the theme toggle button lives in the top bar, visible
    // in every screenshot in this file, but this one exists specifically as
    // the "Settings/theme" control's evidence shot.
    {
        let mut app = seeded_app();
        app.config.ui.theme = theme;
        let mut h = harness(app);
        h.run();
        snapshot(&mut h, &format!("servers_top_bar_theme_toggle{suffix}"));
    }

    // Digest (default landing lens) — every section populated.
    {
        let overdue = {
            let mut t = sample_task("TASK-1", "Overdue task", "To Do");
            t.created_date = Some("2020-01-01 00:00".to_string());
            t
        };
        let in_progress = sample_task("TASK-2", "In-flight task", "In Progress");
        let done = {
            let mut t = sample_task("TASK-3", "Done task", "Done");
            t.updated_date = Some("2026-08-01 12:00".to_string());
            t
        };
        let app = app_with(theme, BacklogLens::Digest, vec![overdue, in_progress, done]);
        let mut h = harness(app);
        h.run();
        snapshot(&mut h, &format!("backlog_digest{suffix}"));
    }

    // TASK-99: the Digest *place* (mock §1) — goal cards, "In flight", and
    // every "Needs a human" row type populated at once (failed run, stalled
    // run, port squatter, removable worktree).
    {
        let app = digest_place_app(theme);
        let repo_root = PathBuf::from(REPO_PATH);
        let dispatching = task_with(
            sample_task("TASK-83", "Dispatching now", "To Do"),
            &[DISPATCHING_LABEL],
            "",
        );
        let in_progress = sample_task("TASK-76", "In-progress task", "In Progress");
        let failed = task_with(
            sample_task("TASK-61", "Failed dispatch run", "To Do"),
            &[DISPATCH_FAILED_LABEL],
            "Dispatch failed: agent error after 22m",
        );
        let stalled = task_with(
            sample_task("TASK-90", "Stalled dispatch run", "To Do"),
            &[DISPATCHING_LABEL],
            "",
        );
        app.backlog_repos.lock().unwrap().insert(
            repo_root.clone(),
            BacklogRepo {
                root: repo_root.clone(),
                tasks: vec![dispatching, in_progress, failed, stalled],
                warnings: vec![],
                project_defs: vec![],
                initiative_defs: vec![],
                goals: vec![
                    goal_def("Close out Stack Ranking", 4, 1),
                    goal_def("Dispatch throughput", 8, 7),
                ],
                ranking: switchbard_core::RepoRanking::default(),
                loaded_at_unix: 0,
                configured_statuses: vec![
                    "To Do".into(),
                    "In Progress".into(),
                    "In Review".into(),
                    "Done".into(),
                ],
            },
        );
        app.dispatch_runs.lock().unwrap().insert(
            (repo_root.clone(), "TASK-90".to_string()),
            switchbard_core::dispatch_inspect::DispatchRun {
                task_id: "TASK-90".to_string(),
                branch: "dispatch/task-90".to_string(),
                worktree_path: repo_root.join(".worktrees/dispatch-task-90"),
                worktree_exists: false,
                log_path: Some(PathBuf::from("/tmp/switchbard-logs/dispatch-task-90.log")),
                prompt_path: None,
                started_at_unix: Some(
                    switchbard_core::dispatch_inspect::now_unix().saturating_sub(
                        switchbard_core::DispatchOptions::default()
                            .stale_after
                            .as_secs()
                            + 300,
                    ),
                ),
                log_bytes: 0,
                log_modified_unix: None,
                liveness: switchbard_core::dispatch_inspect::DispatchRunLiveness::Alive {
                    pgid: 4242,
                    supervised: true,
                },
                progress: switchbard_core::dispatch_inspect::RunProgress::default(),
            },
        );
        app.state
            .lock()
            .unwrap()
            .listeners
            .push(AttributedListener {
                listener: LocalListener {
                    pid: 4242,
                    pgid: 9001,
                    port: 5173,
                    command_name: "node".to_string(),
                    cwd: None,
                },
                repo_name: None,
                worktree_path: None,
                worktree_branch: None,
            });
        let linked = PathBuf::from(format!("{REPO_PATH}-linked"));
        app.worktrees.lock().unwrap().push(WorktreeRef {
            repo_name: REPO_NAME.to_string(),
            path: linked.clone(),
            branch: Some("feat/goal-modal".to_string()),
            head: "abc9999".to_string(),
        });
        app.meta.lock().unwrap().insert(
            linked,
            WorktreeMeta {
                dirty_files: Some(vec![]),
                lock: Fact::Known(None),
                staleness: Some(WorktreeStaleness::Merged {
                    base: "main".to_string(),
                    evidence: LandedEvidence::Ancestry,
                }),
                ..Default::default()
            },
        );
        let mut h = harness(app);
        h.run();
        snapshot(&mut h, &format!("digest_place_populated{suffix}"));
    }

    // TASK-99 mock §7a: the Digest place's zero-goal empty state — "No
    // goals this week" / "+ New goal" / "Roll last week", with In flight and
    // the attention feed both also empty (a genuinely quiet day).
    {
        let app = digest_place_app(theme);
        let mut h = harness(app);
        h.run();
        snapshot(&mut h, &format!("digest_place_zero_goals{suffix}"));
    }

    // List lens with a fully-populated task selected in the detail pane.
    {
        let mut app = app_with(
            theme,
            BacklogLens::List,
            vec![sample_task("TASK-1", "Full detail task", "In Progress")],
        );
        app.backlog_view.selected_task = Some((PathBuf::from(REPO_PATH), "TASK-1".to_string()));
        let mut h = harness(app);
        h.run();
        snapshot(&mut h, &format!("backlog_list_and_detail{suffix}"));
    }

    // Narrow-window stress: the top chrome wraps and the minimum-width
    // detail rail keeps every field usable instead of collapsing controls.
    {
        let mut app = app_with(
            theme,
            BacklogLens::List,
            vec![sample_task("TASK-1", "Narrow detail task", "In Progress")],
        );
        app.backlog_view.selected_task = Some((PathBuf::from(REPO_PATH), "TASK-1".to_string()));
        let mut h = harness(app);
        h.set_size(egui::vec2(900.0, 700.0));
        // Panel heights change after the harness's initial 1280px build;
        // give the resized viewport a few complete layout/paint frames.
        h.run_steps(3);
        snapshot(&mut h, &format!("backlog_narrow_window{suffix}"));
    }

    // Interactive-state evidence: focus the global filter while hovering
    // Refresh, proving the centralized focus/hover styling in both themes.
    {
        let mut app = seeded_app();
        app.config.ui.theme = theme;
        // TASK-96: the top bar's filter row (holding the global filter
        // input this shot exists to prove) only renders for Ops/Tasks now
        // — `seeded_app()`'s default `Place::Digest` has no filter row at
        // all, which is what this screenshot is not testing.
        app.place = Place::Ops;
        let mut h = harness(app);
        h.run();
        h.query_all(kittest::by().role(egui::accesskit::Role::TextInput))
            .next()
            .expect("global filter input")
            .focus();
        h.get_by_label("Refresh").hover();
        h.run();
        snapshot(&mut h, &format!("servers_focus_hover{suffix}"));
    }

    // Board lens.
    {
        let app = app_with(
            theme,
            BacklogLens::Board,
            vec![
                sample_task("TASK-1", "To do card", "To Do"),
                sample_task("TASK-2", "In progress card", "In Progress"),
            ],
        );
        let mut h = harness(app);
        h.run();
        snapshot(&mut h, &format!("backlog_board{suffix}"));
    }

    // Board with the task-detail rail collapsed to its edge toggle.
    {
        let mut app = app_with(
            theme,
            BacklogLens::Board,
            vec![sample_task("TASK-1", "Board task", "To Do")],
        );
        app.backlog_view.detail_rail_collapsed = true;
        let mut h = harness(app);
        h.run();
        snapshot(&mut h, &format!("backlog_board_rail_collapsed{suffix}"));
    }

    // Milestones lens.
    {
        let app = app_with(
            theme,
            BacklogLens::Projects,
            vec![sample_task("TASK-1", "Milestoned task", "To Do")],
        );
        let mut h = harness(app);
        h.run();
        snapshot(&mut h, &format!("backlog_milestones{suffix}"));
    }

    // Portfolio lens.
    {
        let app = app_with(
            theme,
            BacklogLens::Portfolio,
            vec![sample_task("TASK-1", "Portfolio task", "To Do")],
        );
        let mut h = harness(app);
        h.run();
        snapshot(&mut h, &format!("backlog_portfolio{suffix}"));
    }

    // Statistics lens.
    {
        let app = app_with(
            theme,
            BacklogLens::Statistics,
            vec![
                sample_task("TASK-1", "Stat task one", "To Do"),
                sample_task("TASK-2", "Stat task two", "Done"),
            ],
        );
        let mut h = harness(app);
        h.run();
        snapshot(&mut h, &format!("backlog_statistics{suffix}"));
    }

    // Done task detail pane, offering "Complete" instead of "Archive"
    // (2026-08-05 fix wave 2 — Backlog.md semantics: a Done task is
    // completed into backlog/completed/, not archived).
    {
        let mut app = app_with(
            theme,
            BacklogLens::List,
            vec![sample_task("TASK-1", "Finished task", "Done")],
        );
        // Done tasks are hidden by default; without this the selection
        // reconciles away and the detail pane shows nothing (the exact
        // gotcha the fix wave's own tests document).
        app.backlog_view.show_completed = true;
        app.backlog_view.selected_task = Some((PathBuf::from(REPO_PATH), "TASK-1".to_string()));
        app.backlog_view.archive_confirm = true;
        let mut h = harness(app);
        h.run();
        snapshot(
            &mut h,
            &format!("backlog_done_task_offers_complete{suffix}"),
        );
    }

    // "Clean Up Old Tasks" confirm state, showing "Complete N Done tasks?"
    // (fix wave 2 — was "Archive N Done tasks?" before the real CLI's
    // refusal of `task archive` on a Done task was discovered and fixed).
    {
        let app = app_with(
            theme,
            BacklogLens::List,
            vec![sample_task("TASK-1", "Finished task", "Done")],
        );
        let mut h = harness(app);
        h.run();
        h.state_mut().backlog_view.cleanup_confirm = true;
        h.run();
        snapshot(
            &mut h,
            &format!("backlog_cleanup_confirm_complete_wording{suffix}"),
        );
    }

    // Create modal, open.
    {
        let mut app = app_with(
            theme,
            BacklogLens::List,
            vec![sample_task("TASK-1", "Existing task", "To Do")],
        );
        app.backlog_view.new_task.open = true;
        app.backlog_view.new_task.target_repo = Some(PathBuf::from(REPO_PATH));
        let mut h = harness(app);
        h.run();
        snapshot(&mut h, &format!("backlog_create_modal{suffix}"));
    }

    // Create modal as opened from the In Progress Board column: the Board
    // stays in context and the destination status is already selected.
    {
        let mut app = app_with(
            theme,
            BacklogLens::Board,
            vec![sample_task("TASK-1", "Existing board task", "To Do")],
        );
        app.backlog_view.new_task.open = true;
        app.backlog_view.new_task.target_repo = Some(PathBuf::from(REPO_PATH));
        app.backlog_view.new_task.status = "In Progress".to_string();
        let mut h = harness(app);
        h.run();
        snapshot(&mut h, &format!("backlog_board_column_create{suffix}"));
    }

    // Global search overlay, with a live query and a match.
    {
        let mut app = app_with(
            theme,
            BacklogLens::List,
            vec![sample_task("TASK-1", "Searchable task", "To Do")],
        );
        app.backlog_view.search.open = true;
        app.backlog_view.search.query = "Searchable".to_string();
        let mut h = harness(app);
        h.run();
        snapshot(&mut h, &format!("backlog_search_overlay{suffix}"));
    }

    // Saved views bar with a saved view active (Statistics lens, matching
    // ui_views.rs's own choice, to avoid an ambiguous second "Save" button).
    {
        let mut app = app_with(
            theme,
            BacklogLens::Statistics,
            vec![sample_task("TASK-1", "Task", "To Do")],
        );
        app.config
            .ui
            .saved_views
            .push(switchbard_core::config::SavedView {
                name: "High priority".to_string(),
                selected_repo: None,
                status_filter: "all".to_string(),
                priority_filter: "high".to_string(),
                project_filter: "all".to_string(),
                label_filter: "all".to_string(),
                sort_key: String::new(),
                sort_direction: String::new(),
                lens: "statistics".to_string(),
                show_completed: false,
                show_archived: false,
                show_drafts: true,
                tasks_filters: Vec::new(),
                tasks_group_by: String::new(),
                tasks_view_mode: String::new(),
            });
        app.backlog_view.active_saved_view = Some("High priority".to_string());
        let mut h = harness(app);
        h.run();
        snapshot(&mut h, &format!("backlog_saved_views_bar{suffix}"));
    }

    // Dispatch affordances: one screenshot per state, each as the selected
    // detail-pane task so the pill + message/button combination is visible.
    for (state_name, labels, notes) in [
        ("not_flagged", vec![], String::new()),
        ("queued", vec![DISPATCH_LABEL.to_string()], String::new()),
        (
            "in_flight",
            vec![DISPATCHING_LABEL.to_string()],
            String::new(),
        ),
        (
            "dispatched",
            vec![DISPATCHED_LABEL.to_string()],
            "Dispatch PR: https://github.com/example/switchbard/pull/7".to_string(),
        ),
        (
            "failed",
            vec![DISPATCH_FAILED_LABEL.to_string()],
            "Dispatch failed: headless run exited with status 1".to_string(),
        ),
    ] {
        let mut task = sample_task("TASK-1", "Dispatch state task", "To Do");
        task.labels = labels;
        task.implementation_notes = notes;
        let mut app = app_with(theme, BacklogLens::List, vec![task]);
        app.backlog_view.selected_task = Some((PathBuf::from(REPO_PATH), "TASK-1".to_string()));
        let mut h = harness(app);
        h.run();
        snapshot(&mut h, &format!("backlog_dispatch_{state_name}{suffix}"));
    }

    // TASK-101 (IA V2 Goals place): the index (pace chips, inline check-in,
    // "automatic" for a measured goal, edit-target) and the goal page
    // (this-week/history/Inputs cards) — mock §5's frozen reference.
    {
        let week = switchbard_core::week_monday_of(chrono::Local::now().date_naive())
            .format("%Y-%m-%d")
            .to_string();
        let behind_goal = switchbard_core::GoalDef {
            name: "Close out Stack Ranking".to_string(),
            unit: "tasks".to_string(),
            measure: switchbard_core::GoalMeasure::Tasks,
            scope: Some("Stack Ranking".to_string()),
            inputs: switchbard_core::GoalInputs {
                tasks: vec!["TASK-61".to_string()],
                projects: vec!["Stack Ranking".to_string()],
            },
            weeks: std::collections::BTreeMap::from([(
                week.clone(),
                switchbard_core::GoalWeek {
                    target: 4,
                    checkins: vec![],
                },
            )]),
        };
        let met_goal = switchbard_core::GoalDef {
            name: "Dispatch throughput".to_string(),
            unit: "tasks".to_string(),
            measure: switchbard_core::GoalMeasure::Tasks,
            scope: None,
            inputs: switchbard_core::GoalInputs::default(),
            weeks: std::collections::BTreeMap::from([(
                week.clone(),
                switchbard_core::GoalWeek {
                    target: 8,
                    checkins: vec![],
                },
            )]),
        };
        let manual_goal = switchbard_core::GoalDef {
            name: "IA decision record".to_string(),
            unit: "docs".to_string(),
            measure: switchbard_core::GoalMeasure::Manual,
            scope: None,
            inputs: switchbard_core::GoalInputs::default(),
            weeks: std::collections::BTreeMap::from([(
                week,
                switchbard_core::GoalWeek {
                    target: 1,
                    checkins: vec![],
                },
            )]),
        };
        let mut task_61 = sample_task(
            "TASK-61",
            "Landing worker: gh probe has no timeout",
            "To Do",
        );
        task_61.project = None;
        let mut task_70 = sample_task("TASK-70", "Freshly created task sometimes missing", "Done");
        task_70.project = Some("Stack Ranking".to_string());
        // Done *this* week, not `sample_task`'s fixed June date — otherwise
        // the Inputs card's "done this week" count would read 0 despite a
        // Done member task, which is misleading in a showcase screenshot.
        task_70.updated_date = Some(format!(
            "{} 12:00",
            switchbard_core::week_monday_of(chrono::Local::now().date_naive()).format("%Y-%m-%d")
        ));

        let mut repo = project_with(vec![task_61.clone(), task_70]);
        repo.goals = vec![behind_goal, met_goal, manual_goal];

        let mut app = seeded_app();
        app.config.ui.theme = theme;
        app.place = Place::Goals;
        app.backlog_repos
            .lock()
            .unwrap()
            .insert(PathBuf::from(REPO_PATH), repo);
        let mut h = harness(app);
        h.run();
        snapshot(&mut h, &format!("goals_index{suffix}"));

        h.state_mut().goals_view.selected_goal = Some((
            PathBuf::from(REPO_PATH),
            "Close out Stack Ranking".to_string(),
        ));
        h.run();
        snapshot(&mut h, &format!("goals_goal_page{suffix}"));

        // Narrow-width stress (mock §7d): sidebar collapses to the icon
        // rail; the index table still fits.
        h.state_mut().goals_view.selected_goal = None;
        h.set_size(egui::vec2(600.0, 700.0));
        h.run_steps(3);
        snapshot(&mut h, &format!("goals_index_narrow{suffix}"));
    }

    // TASK-96 (IA V2 sidebar shell): the expanded places nav, with a
    // repo scope narrowed to one repo and every favorite kind populated so
    // the FAVORITES group, the scope selector's "1 repo" label, and every
    // place's count badge all have something real to show.
    {
        let mut app = app_with(
            theme,
            BacklogLens::Digest,
            vec![sample_task("TASK-1", "Nav screenshot task", "In Progress")],
        );
        app.place = Place::Digest;
        app.repo_scope = std::iter::once(PathBuf::from(REPO_PATH)).collect();
        app.config.ui.favorites = vec![
            switchbard_core::config::FavoriteRef {
                kind: switchbard_core::config::FavoriteKind::Project,
                repo: REPO_PATH.to_string(),
                key: "Stack Ranking".to_string(),
            },
            switchbard_core::config::FavoriteRef {
                kind: switchbard_core::config::FavoriteKind::Task,
                repo: REPO_PATH.to_string(),
                key: "TASK-1".to_string(),
            },
            switchbard_core::config::FavoriteRef {
                kind: switchbard_core::config::FavoriteKind::Goal,
                repo: REPO_PATH.to_string(),
                key: "Dispatch throughput".to_string(),
            },
        ];
        let mut h = harness(app);
        h.run();
        snapshot(&mut h, &format!("nav_expanded{suffix}"));
    }

    // TASK-96 narrow-width icon rail (mock §4/§7d).
    {
        let app = app_with(theme, BacklogLens::Digest, vec![]);
        let mut h = harness(app);
        h.set_size(egui::vec2(600.0, 700.0));
        h.run();
        snapshot(&mut h, &format!("nav_rail_narrow{suffix}"));
    }

    // TASK-100: the merged Ops table — two repos (one primary + one linked
    // worktree each), a detected-but-idle service, a switchbard-started
    // running service, a dispatch run's Agent-cell attribution, and an
    // external squatter listener row at the bottom, matching mock §6.
    {
        let app = ops_screenshot_app(theme);
        let mut h = harness(app);
        h.run();
        snapshot(&mut h, &format!("ops_table_populated{suffix}"));
    }

    // TASK-100: the bulk-select stroke-ring convention, on the same
    // populated fixture — a dedicated shot because `paint_selection_ring`'s
    // `debug_painter` overlay approach (see its doc in `ops::row`) is worth
    // eyeballing for z-order glitches a unit test can't catch.
    {
        let mut app = ops_screenshot_app(theme);
        app.bulk_selected_worktrees
            .insert(PathBuf::from(format!("{REPO_PATH}/../switchbard-feature")));
        let mut h = harness(app);
        h.run();
        snapshot(&mut h, &format!("ops_table_selected_row{suffix}"));
    }

    // TASK-100 design-state matrix: narrow width. The nav collapses to its
    // icon rail below 720px (mock §7d, already covered by `nav_rail_narrow`
    // above); this shot proves the Ops table itself stays usable alongside
    // it rather than overflowing or clipping silently.
    {
        let app = ops_screenshot_app(theme);
        let mut h = harness(app);
        h.set_size(egui::vec2(600.0, 700.0));
        h.run_steps(3);
        snapshot(&mut h, &format!("ops_table_narrow{suffix}"));
    }

    // TASK-100 medic pass — MAJOR: a row busy enough to need more chips than
    // fit used to `horizontal_wrapped` onto a second line, painting straight
    // through the table's fixed `ROW_HEIGHT` into the row below it
    // (2026-09-01 screenshot review). This is the visual half of the fix's
    // evidence (the mechanical half is `ops_table_rows.rs`'s
    // `a_busy_row_caps_visible_chips_instead_of_wrapping_onto_a_second_line`)
    // — 5 services + 3 listeners on one worktree, proving the capped,
    // single-line chip rows plus `.clip(true)` never paint outside their own
    // row band, whatever sits above or below it.
    {
        let app = ops_screenshot_app_with_busy_row(theme);
        let mut h = harness(app);
        h.run();
        snapshot(&mut h, &format!("ops_table_busy_row{suffix}"));
    }
}

/// Two repos, each with a primary + one linked worktree; the primary of the
/// first repo has a detected (idle) service and a switchbard-started running
/// one, the linked worktree of the first repo holds a live dispatch run
/// (Agent cell attribution), and one listener is left unattributed (the
/// bottom squatter row). Deliberately not `common::seeded_app()` — this
/// needs two repos and richer per-worktree state than that fixture carries.
fn ops_screenshot_app(theme: ThemeChoice) -> HiveApp {
    use std::path::PathBuf;
    use std::time::Instant;
    use switchbard_core::config::Config;
    use switchbard_core::dispatch_inspect::{DispatchRun, DispatchRunLiveness, RunProgress};
    use switchbard_core::types::LocalListener;
    use switchbard_core::{
        AttributedListener, DetectedService, Repo, ServerLikelihood, ServiceSource, WorktreeRef,
    };
    use switchbard_gui::runtime::{ActiveRun, Place, WorktreeMeta};

    const REPO_B_NAME: &str = "lucella";
    const REPO_B_PATH: &str = "/tmp/switchbard-ui-test/lucella";
    let linked_path = PathBuf::from(format!("{REPO_PATH}/../switchbard-feature"));

    let repos = vec![
        Repo {
            name: REPO_NAME.to_string(),
            path: PathBuf::from(REPO_PATH),
        },
        Repo {
            name: REPO_B_NAME.to_string(),
            path: PathBuf::from(REPO_B_PATH),
        },
    ];
    let worktrees = vec![
        WorktreeRef {
            repo_name: REPO_NAME.to_string(),
            path: PathBuf::from(REPO_PATH),
            branch: Some("main".to_string()),
            head: "abc1234".to_string(),
        },
        WorktreeRef {
            repo_name: REPO_NAME.to_string(),
            path: linked_path.clone(),
            branch: Some("feature/stack-ranking-core".to_string()),
            head: "def5678".to_string(),
        },
        WorktreeRef {
            repo_name: REPO_B_NAME.to_string(),
            path: PathBuf::from(REPO_B_PATH),
            branch: Some("main".to_string()),
            head: "aaaa9999".to_string(),
        },
    ];

    let mut cfg = Config::default();
    cfg.ui.onboarding_dismissed = true;
    cfg.repos = repos.clone();
    let mut app = HiveApp::new_headless(cfg, repos, worktrees);
    app.config_save_path = Some(common::isolated_config_save_path());
    app.config.ui.theme = theme;
    app.place = Place::Ops;

    app.meta.lock().unwrap().insert(
        PathBuf::from(REPO_PATH),
        WorktreeMeta {
            dirty_files: Some(vec![" M src/lib.rs".to_string()]),
            trunk: Some(switchbard_core::TrunkDivergence {
                base: "origin/main".to_string(),
                unlanded: 1,
                ancestry_ahead: 1,
                behind: 0,
            }),
            ..Default::default()
        },
    );

    app.services.lock().unwrap().insert(
        PathBuf::from(REPO_PATH),
        vec![
            DetectedService {
                name: "gui".to_string(),
                command: "cargo run -p switchbard-gui".to_string(),
                cwd_rel: PathBuf::from("."),
                source: ServiceSource::Makefile,
                source_file: PathBuf::from("Makefile"),
                likelihood: ServerLikelihood::Server,
                expected_port: None,
            },
            DetectedService {
                name: "task-cli".to_string(),
                command: "cargo run -p switchbard-task".to_string(),
                cwd_rel: PathBuf::from("."),
                source: ServiceSource::Makefile,
                source_file: PathBuf::from("Makefile"),
                likelihood: ServerLikelihood::Server,
                expected_port: None,
            },
        ],
    );

    app.services.lock().unwrap().insert(
        PathBuf::from(REPO_B_PATH),
        vec![
            DetectedService {
                name: "vite".to_string(),
                command: "npm run dev".to_string(),
                cwd_rel: PathBuf::from("."),
                source: ServiceSource::NodeScript,
                source_file: PathBuf::from("package.json"),
                likelihood: ServerLikelihood::Server,
                expected_port: Some(5173),
            },
            DetectedService {
                name: "api".to_string(),
                command: "npm run api".to_string(),
                cwd_rel: PathBuf::from("."),
                source: ServiceSource::NodeScript,
                source_file: PathBuf::from("package.json"),
                likelihood: ServerLikelihood::Server,
                expected_port: Some(8787),
            },
        ],
    );

    app.active_runs.lock().unwrap().insert(
        5173,
        ActiveRun {
            worktree_path: PathBuf::from(REPO_B_PATH),
            service_name: "vite".to_string(),
            command: "npm run dev".to_string(),
            pid: 5173,
            pgid: 5173,
            started_at: Instant::now(),
            log_path: PathBuf::from("/tmp/switchbard-logs/lucella-vite.log"),
        },
    );
    app.state
        .lock()
        .unwrap()
        .listeners
        .push(AttributedListener {
            listener: LocalListener {
                pid: 5173,
                pgid: 5173,
                port: 5173,
                command_name: "vite".to_string(),
                cwd: Some(PathBuf::from(REPO_B_PATH)),
            },
            repo_name: Some(REPO_B_NAME.to_string()),
            worktree_path: Some(PathBuf::from(REPO_B_PATH)),
            worktree_branch: Some("main".to_string()),
        });
    app.state
        .lock()
        .unwrap()
        .listeners
        .push(AttributedListener {
            listener: LocalListener {
                pid: 8787,
                pgid: 8787,
                port: 8787,
                command_name: "api".to_string(),
                cwd: Some(PathBuf::from(REPO_B_PATH)),
            },
            repo_name: Some(REPO_B_NAME.to_string()),
            worktree_path: Some(PathBuf::from(REPO_B_PATH)),
            worktree_branch: Some("main".to_string()),
        });

    // The linked worktree's live dispatch run — the Agent cell's "claude ·
    // active …" attribution (TASK-100; see `ui::places::ops::agent`).
    app.dispatch_runs.lock().unwrap().insert(
        (PathBuf::from(REPO_PATH), "TASK-1".to_string()),
        DispatchRun {
            task_id: "TASK-1".to_string(),
            branch: "feature/stack-ranking-core".to_string(),
            worktree_path: linked_path,
            worktree_exists: true,
            log_path: None,
            prompt_path: None,
            // An hour ago, not the Unix epoch — `humanize_age` renders this
            // as "active 1h" (matching the mock's own example) rather than
            // "56y ago", which is what a literal `Some(0)` produced here
            // before the 2026-09-01 screenshot review caught it.
            started_at_unix: Some(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs()
                    .saturating_sub(3600),
            ),
            log_bytes: 0,
            log_modified_unix: None,
            liveness: DispatchRunLiveness::Alive {
                pgid: 9001,
                supervised: true,
            },
            progress: RunProgress::default(),
        },
    );

    // External squatter — attributed to no worktree (mock §6's bottom row).
    app.state
        .lock()
        .unwrap()
        .listeners
        .push(AttributedListener {
            listener: LocalListener {
                pid: 4821,
                pgid: 4821,
                port: 3000,
                command_name: "node".to_string(),
                cwd: None,
            },
            repo_name: None,
            worktree_path: None,
            worktree_branch: None,
        });

    app
}

/// `ops_screenshot_app` plus 5 services + 3 listeners piled onto its linked
/// worktree — TASK-100 medic pass fixture for `ops_table_busy_row`, proving
/// the capped Services/Listening cells stay inside their row band instead of
/// wrapping into the row below (the bug the 2026-09-01 screenshot review
/// flagged).
fn ops_screenshot_app_with_busy_row(theme: ThemeChoice) -> HiveApp {
    use switchbard_core::types::LocalListener;
    use switchbard_core::{AttributedListener, DetectedService, ServerLikelihood, ServiceSource};

    let app = ops_screenshot_app(theme);
    let linked_path = PathBuf::from(format!("{REPO_PATH}/../switchbard-feature"));

    let mut services = app
        .services
        .lock()
        .unwrap()
        .get(&linked_path)
        .cloned()
        .unwrap_or_default();
    for name in ["svc-a", "svc-b", "svc-c", "svc-d", "svc-e"] {
        services.push(DetectedService {
            name: name.to_string(),
            command: format!("cargo run -p {name}"),
            cwd_rel: PathBuf::from("."),
            source: ServiceSource::Makefile,
            source_file: PathBuf::from("Makefile"),
            likelihood: ServerLikelihood::Server,
            expected_port: None,
        });
    }
    app.services
        .lock()
        .unwrap()
        .insert(linked_path.clone(), services);

    for (i, port) in [7001u16, 7002, 7003].into_iter().enumerate() {
        app.state
            .lock()
            .unwrap()
            .listeners
            .push(AttributedListener {
                listener: LocalListener {
                    pid: 7000 + i as u32,
                    pgid: 7000 + i as i32,
                    port,
                    command_name: format!("worker-{i}"),
                    cwd: Some(linked_path.clone()),
                },
                repo_name: Some(REPO_NAME.to_string()),
                worktree_path: Some(linked_path.clone()),
                worktree_branch: Some("feature/stack-ranking-core".to_string()),
            });
    }

    app
}

#[test]
#[ignore = "wgpu image snapshot: machine-specific, run explicitly with `-- --ignored` (see module docs)"]
fn generate_qa_screenshots() {
    shots_for_theme(ThemeChoice::Light);
    shots_for_theme(ThemeChoice::Dark);
}
