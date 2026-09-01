//! TASK-97 QA evidence — pixel screenshots of the Tasks place (List grouped
//! by project, a group header expanded in place, and Board view mode), in
//! both themes, for `docs/qa/screenshots/`. Same machinery and posture as
//! `qa_screenshots.rs` (`wgpu`, real GPU adapter, `#[ignore]`d — pixel
//! output is GPU/driver/font sensitive, not part of the CI gate; this file
//! isn't a regression baseline, it exists to produce human-reviewable
//! evidence images).
//!
//! Regenerate with:
//! ```sh
//! UPDATE_SNAPSHOTS=1 cargo test -p switchbard-gui --test qa_screenshots_tasks_place -- --ignored
//! ```

mod common;

use std::path::{Path, PathBuf};

use common::{harness, seeded_app, REPO_PATH};
use eframe::egui;
use egui_kittest::{Harness, SnapshotOptions};
use switchbard_core::config::ThemeChoice;
use switchbard_core::{
    BacklogChecklistItem, BacklogRepo, BacklogTaskSource, GoalDef, GoalInputs, GoalMeasure,
    GoalWeek, ProjectDef,
};
use switchbard_gui::app::HiveApp;
use switchbard_gui::runtime::{Place, TasksView};

fn output_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/qa/screenshots")
}

fn snapshot(harness: &mut Harness<'_, HiveApp>, name: &str) {
    let options = SnapshotOptions::new().output_path(output_dir());
    let _ = harness.try_snapshot_options(name, &options);
}

fn task(
    id: &str,
    title: &str,
    status: &str,
    project: Option<&str>,
) -> switchbard_core::BacklogTask {
    switchbard_core::BacklogTask {
        id: id.to_string(),
        title: title.to_string(),
        status: status.to_string(),
        priority: "medium".to_string(),
        assignees: vec!["ben".to_string()],
        labels: vec!["demo".to_string()],
        dependencies: vec![],
        references: vec![],
        project: project.map(str::to_string),
        parent: None,
        created_date: Some("2026-06-01 09:00".to_string()),
        updated_date: Some("2026-06-20 12:00".to_string()),
        description: String::new(),
        implementation_plan: String::new(),
        implementation_notes: String::new(),
        final_summary: String::new(),
        acceptance_criteria: vec![BacklogChecklistItem {
            index: 1,
            checked: false,
            text: "Criterion".to_string(),
        }],
        definition_of_done: vec![],
        source: BacklogTaskSource::Active,
        path: PathBuf::from(format!(
            "{REPO_PATH}/backlog/tasks/{}.md",
            id.to_lowercase()
        )),
    }
}

fn repo_with(
    tasks: Vec<switchbard_core::BacklogTask>,
    project_defs: Vec<ProjectDef>,
    goals: Vec<GoalDef>,
) -> BacklogRepo {
    BacklogRepo {
        root: PathBuf::from(REPO_PATH),
        tasks,
        warnings: vec![],
        project_defs,
        initiative_defs: vec![],
        goals,
        ranking: switchbard_core::RepoRanking::default(),
        loaded_at_unix: 0,
        configured_statuses: vec![
            "To Do".into(),
            "In Progress".into(),
            "In Review".into(),
            "Done".into(),
        ],
    }
}

/// A realistic fixture: two projects (one with a def + a goal attached, one
/// without a def at all — "referenced only"), a "No project" bucket, a
/// sub-issue pair, and a long title to exercise the two-line clamp (mock
/// §7c).
fn fixture_repo() -> BacklogRepo {
    let long_title = "A deliberately long task title that should clamp at two \
        lines and never grow the row, matching the mock's §7c stress state \
        for unbroken and long content"
        .to_string();
    let mut sub = task(
        "TASK-4.1",
        "Sub-issue of TASK-4",
        "To Do",
        Some("Stack Ranking"),
    );
    sub.parent = Some("TASK-4".to_string());

    repo_with(
        vec![
            task(
                "TASK-1",
                "Ship the rank CLI",
                "In Progress",
                Some("Stack Ranking"),
            ),
            task("TASK-2", "GUI sort by rank", "To Do", Some("Stack Ranking")),
            task("TASK-3", "Reorder controls", "Done", Some("Stack Ranking")),
            task("TASK-4", &long_title, "To Do", Some("Stack Ranking")),
            sub,
            task("TASK-5", "Lavish mockup", "In Progress", Some("IA V2")),
            task("TASK-6", "No project task", "To Do", None),
        ],
        vec![ProjectDef {
            name: "Stack Ranking".to_string(),
            status: "In Progress".to_string(),
            target_date: None,
            initiative: None,
            lead: None,
            description: "One global rank order across the backlog.".to_string(),
            path: PathBuf::from(format!("{REPO_PATH}/backlog/projects/Stack-Ranking.md")),
        }],
        vec![GoalDef {
            name: "Close out Stack Ranking".to_string(),
            unit: "tasks".to_string(),
            measure: GoalMeasure::Tasks,
            scope: Some("Stack Ranking".to_string()),
            inputs: GoalInputs::default(),
            weeks: std::collections::BTreeMap::from([(
                switchbard_core::week_monday_of(chrono::Local::now().date_naive())
                    .format("%Y-%m-%d")
                    .to_string(),
                GoalWeek {
                    target: 4,
                    checkins: vec![],
                },
            )]),
        }],
    )
}

fn app_with(theme: ThemeChoice) -> HiveApp {
    let mut app = seeded_app();
    app.config.ui.theme = theme;
    app.place = Place::Tasks;
    app.tasks_view = TasksView::All;
    app.backlog_repos
        .lock()
        .unwrap()
        .insert(PathBuf::from(REPO_PATH), fixture_repo());
    app
}

fn shots_for_theme(theme: ThemeChoice) {
    let suffix = format!("_{theme:?}").to_lowercase();

    // List, grouped by project (the default) — group headers with computed
    // roll-ups, the "No project" bucket, the sub-issue indented in place,
    // and the long-title two-line clamp.
    {
        let app = app_with(theme);
        let mut h = harness(app);
        h.run();
        snapshot(&mut h, &format!("tasks_place_list_grouped{suffix}"));
    }

    // A group header expanded in place — the cut project page's
    // replacement (mock §3): remaining count, progress meter, goal-pace
    // chip, description.
    {
        let mut app = app_with(theme);
        app.tasks_place
            .expanded_groups
            .insert("Stack Ranking".to_string());
        let mut h = harness(app);
        h.run();
        snapshot(&mut h, &format!("tasks_place_header_expanded{suffix}"));
    }

    // Board view mode — same facets, status-ordered columns.
    {
        let mut app = app_with(theme);
        app.tasks_place.view_mode = switchbard_gui::ui::places::tasks::state::TasksViewMode::Board;
        let mut h = harness(app);
        h.run();
        snapshot(&mut h, &format!("tasks_place_board{suffix}"));
    }

    // Narrow width: the sidebar collapses to the icon rail (mock §7d) and
    // the facets row wraps.
    {
        let app = app_with(theme);
        let mut h = harness(app);
        h.set_size(egui::vec2(700.0, 800.0));
        h.run();
        snapshot(&mut h, &format!("tasks_place_narrow{suffix}"));
    }
}

#[test]
#[ignore = "pixel screenshots — GPU/driver/font sensitive, run explicitly (see module doc)"]
fn tasks_place_screenshots_both_themes() {
    shots_for_theme(ThemeChoice::Light);
    shots_for_theme(ThemeChoice::Dark);
}
