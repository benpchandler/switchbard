//! TASK-97 render-path perf smoke (CLAUDE.md's render-path rule — the
//! Tasks place gained per-frame work: the generic group-by engine
//! (`groups::build_groups`), the filter-builder predicate pass, and the
//! flatten-to-a-uniform-row-list virtualization `list_body` does before
//! `egui::ScrollArea::show_rows`).
//!
//! The fixture matches `projects_rank_perf_smoke.rs`'s scale (the design-
//! state matrix's "realistic max N" row): several repos, many projects,
//! many tasks per project, List view mode grouped by Project (the default,
//! and the densest group-by path — it also joins `compute_hierarchy_
//! rollup` and a goal-pace lookup per group). Same harness contract as
//! that test and `workspace_perf_smoke.rs` / `dispatch_chrome_perf_smoke.rs`
//! (headless egui_kittest through the real `render_ui`, `#[ignore]`d — run
//! deliberately, not on every `cargo test`).
//!
//! The Board journey uses the same deterministic 500-task fixture. Before
//! fixed-row virtualization, it measured p50 62.210ms / p95 65.445ms / max
//! 267.848ms over 200 frames and took 14.63s for the test body. After the
//! change it measured p50 11.400ms / p95 13.207ms / max 101.991ms and took
//! 2.23s (2026-09-01, debug build, M-series laptop).
//!
//! ## Measured (2026-09-01, debug build, M-series laptop)
//!
//! 500 tasks (4 repos x 5 projects x 25 tasks/project), List view mode
//! grouped by Project, 200 frames:
//!
//! | | frame p50 | frame p95 | frame max |
//! |---|---|---|---|
//! | this test (virtualized `show_rows`) | 14.270ms | 18.692ms | 130.243ms |
//!
//! For scale comparison, `projects_rank_perf_smoke.rs` measures ~26-28ms
//! p95 on a smaller fixture (200 tasks) through the pre-existing
//! *unvirtualized* Projects lens (its own doc names that as the dominant
//! cost, "TASK-13, which this change does not touch"). This test's lower
//! p95 despite 2.5x the task count is exactly what `show_rows` virtualization
//! buys: only the rows scrolled into view get built each frame, so cost
//! stays roughly flat in the visible viewport height, not linear in total
//! task count — the frame max spike is a one-off compile/GC-adjacent
//! outlier the p50/p95 already show is not representative.
//!
//! Run explicitly:
//! ```sh
//! cargo test -p switchbard-gui --test tasks_place_perf_smoke -- --ignored --nocapture
//! ```

mod common;

use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

use common::{harness, isolated_config_save_path};
use switchbard_core::config::Config;
use switchbard_core::{
    BacklogRepo, BacklogTask, BacklogTaskSource, Repo, RepoRanking, WorktreeRef,
};
use switchbard_gui::app::HiveApp;
use switchbard_gui::runtime::Place;
use switchbard_gui::ui::places::tasks::state::TasksViewMode;

const REPOS: usize = 4;
const PROJECTS_PER_REPO: usize = 5;
const TASKS_PER_PROJECT: usize = 25;
const FRAMES: usize = 200;

/// A ceiling over a debug-build measured baseline, not a target — see
/// `projects_rank_perf_smoke.rs`'s identical note for why this stays a
/// generous headroom check rather than a tight regression gate.
const FRAME_P95_BUDGET_MS: f64 = 40.0;

// Both ignored probes use the renderer's process-wide performance-log
// environment variables. `cargo test -- --ignored` runs them in parallel by
// default, so serialize that small critical section rather than let one test
// replace the other's log path mid-run.
static PERF_ENV_LOCK: Mutex<()> = Mutex::new(());

fn task(repo: usize, project: usize, i: usize) -> BacklogTask {
    let id = format!("TASK-{repo}{project}{i:02}");
    BacklogTask {
        title: format!("Task {i} of project {project} in repo {repo}"),
        // Keep all 500 fixture tasks visible under the Tasks place default,
        // which hides completed work. The Board smoke therefore measures the
        // requested 500 matching rows rather than a smaller post-filter set.
        status: if i.is_multiple_of(2) {
            "In Progress"
        } else {
            "To Do"
        }
        .to_string(),
        priority: "medium".to_string(),
        assignees: vec![],
        labels: if i.is_multiple_of(3) {
            vec!["bug".to_string()]
        } else {
            vec![]
        },
        dependencies: vec![],
        references: vec![],
        project: Some(format!("Project {repo}-{project}")),
        parent: None,
        created_date: Some("2026-06-01 09:00".to_string()),
        updated_date: Some("2026-06-01 09:00".to_string()),
        description: "Lorem ipsum dolor sit amet.".to_string(),
        implementation_plan: String::new(),
        implementation_notes: String::new(),
        final_summary: String::new(),
        acceptance_criteria: vec![],
        definition_of_done: vec![],
        source: BacklogTaskSource::Active,
        path: PathBuf::from(format!("/tmp/switchbard-tasks-place-perf/{id}.md")),
        id,
    }
}

fn build_fixture() -> HiveApp {
    let mut repos = Vec::new();
    let mut worktrees = Vec::new();
    let mut backlog_repos = Vec::new();
    for r in 0..REPOS {
        let repo_path = PathBuf::from(format!("/tmp/switchbard-tasks-place-perf/repo-{r}"));
        let repo_name = format!("repo-{r}");
        repos.push(Repo {
            name: repo_name.clone(),
            path: repo_path.clone(),
        });
        worktrees.push(WorktreeRef {
            repo_name,
            path: repo_path.clone(),
            branch: Some("main".to_string()),
            head: "aaaa1111".to_string(),
        });

        let mut tasks = Vec::new();
        for p in 0..PROJECTS_PER_REPO {
            for i in 0..TASKS_PER_PROJECT {
                tasks.push(task(r, p, i));
            }
        }
        backlog_repos.push((
            repo_path.clone(),
            BacklogRepo {
                root: repo_path,
                tasks,
                warnings: vec![],
                project_defs: vec![],
                initiative_defs: vec![],
                goals: vec![],
                ranking: RepoRanking::default(),
                loaded_at_unix: 0,
                configured_statuses: vec!["To Do".into(), "In Progress".into(), "Done".into()],
            },
        ));
    }

    let mut cfg = Config::default();
    cfg.ui.onboarding_dismissed = true;
    let mut app = HiveApp::new_headless(cfg, repos, worktrees);
    app.config_save_path = Some(isolated_config_save_path());
    app.place = Place::Tasks;
    {
        let mut cached = app.backlog_repos.lock().unwrap();
        for (root, repo) in backlog_repos {
            cached.insert(root, repo);
        }
    }
    app
}

#[derive(Debug, Clone, Copy, Default)]
struct Percentiles {
    p50: f64,
    p95: f64,
    max: f64,
}

fn percentiles(mut values: Vec<f64>) -> Percentiles {
    if values.is_empty() {
        return Percentiles::default();
    }
    values.sort_by(f64::total_cmp);
    let at = |p: f64| -> f64 {
        let rank = ((p / 100.0) * values.len() as f64).ceil() as usize;
        values[rank.saturating_sub(1).min(values.len() - 1)]
    };
    Percentiles {
        p50: at(50.0),
        p95: at(95.0),
        max: *values.last().unwrap(),
    }
}

#[test]
#[ignore = "perf smoke — run explicitly, see module doc"]
fn tasks_place_list_grouped_by_project_perf_smoke() {
    let _perf_env_guard = PERF_ENV_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let log_path = std::env::temp_dir().join(format!(
        "switchbard-tasks-place-perf-smoke-{}.csv",
        std::process::id()
    ));
    // SAFETY (single-threaded-in-practice): one `#[ignore]`d test in this
    // binary, always run standalone via `-- --ignored` — same env-var
    // contract as `projects_rank_perf_smoke.rs`.
    unsafe {
        std::env::set_var("SWITCHBARD_PERF", "1");
        std::env::set_var("SWITCHBARD_PERF_LOG", &log_path);
    }

    let app = build_fixture();
    let mut harness = harness(app);
    for _ in 0..FRAMES {
        harness.run();
    }

    let csv = fs::read_to_string(&log_path)
        .unwrap_or_else(|e| panic!("perf log at {}: {e}", log_path.display()));
    let mut total_ms = Vec::new();
    for line in csv.lines().skip(1) {
        let cols: Vec<&str> = line.split(',').collect();
        if let Some(total) = cols.get(1).and_then(|s| s.parse::<f64>().ok()) {
            total_ms.push(total);
        }
    }
    assert!(
        !total_ms.is_empty(),
        "perf log at {} recorded no frames",
        log_path.display()
    );

    let total = percentiles(total_ms);
    println!(
        "Tasks place List/Project-grouped perf smoke — {REPOS} repos x {PROJECTS_PER_REPO} \
         projects x {TASKS_PER_PROJECT} tasks ({} tasks), {FRAMES} frames:",
        REPOS * PROJECTS_PER_REPO * TASKS_PER_PROJECT,
    );
    println!(
        "  frame  p50 {:.3}ms  p95 {:.3}ms  max {:.3}ms",
        total.p50, total.p95, total.max
    );

    assert!(
        total.p95 < FRAME_P95_BUDGET_MS,
        "frame p95 {:.3}ms exceeds the {FRAME_P95_BUDGET_MS}ms budget — something \
         per-frame-expensive landed on the Tasks place's List/group-by path",
        total.p95
    );
}

#[test]
#[ignore = "perf smoke - run explicitly, see module doc"]
fn tasks_place_board_500_tasks_perf_smoke() {
    let _perf_env_guard = PERF_ENV_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let log_path = std::env::temp_dir().join(format!(
        "switchbard-tasks-place-board-perf-smoke-{}.csv",
        std::process::id()
    ));
    unsafe {
        std::env::set_var("SWITCHBARD_PERF", "1");
        std::env::set_var("SWITCHBARD_PERF_LOG", &log_path);
    }

    let mut app = build_fixture();
    app.tasks_place.view_mode = TasksViewMode::Board;
    let mut harness = harness(app);
    for _ in 0..FRAMES {
        harness.run();
    }

    let csv = fs::read_to_string(&log_path)
        .unwrap_or_else(|e| panic!("perf log at {}: {e}", log_path.display()));
    let total_ms: Vec<f64> = csv
        .lines()
        .skip(1)
        .filter_map(|line| line.split(',').nth(1)?.parse().ok())
        .collect();
    assert!(
        !total_ms.is_empty(),
        "perf log at {} recorded no frames",
        log_path.display()
    );

    let total = percentiles(total_ms);
    println!(
        "Tasks place Board perf smoke - {} tasks, {FRAMES} frames: frame p50 {:.3}ms p95 {:.3}ms max {:.3}ms",
        REPOS * PROJECTS_PER_REPO * TASKS_PER_PROJECT,
        total.p50,
        total.p95,
        total.max,
    );
    assert!(
        total.p95 < FRAME_P95_BUDGET_MS,
        "Board frame p95 {:.3}ms exceeds the {FRAME_P95_BUDGET_MS}ms budget",
        total.p95
    );
}
