//! Stack-ranking render-path perf smoke (CLAUDE.md's render-path rule — the
//! Projects lens gained per-frame work: a per-repo id→position map over
//! `repo.tasks`, per-group row re-sorts by computed order, rank-fact lookups
//! per project group, and two painted arrow buttons plus a possible
//! expedite pill per row).
//!
//! The fixture is the design-state matrix's scale row (state 13): every
//! task ranked, every project ranked, a populated expedite lane — far more
//! rank data than the sparse-by-design feature ever expects, so the
//! measurement bounds the worst case. Same harness contract as
//! `workspace_perf_smoke.rs` / `dispatch_chrome_perf_smoke.rs` (headless
//! egui_kittest through the real `render_ui`, `#[ignore]`d — run
//! deliberately, not on every `cargo test`).
//!
//! ## Measured (2026-09-01, debug build, M-series laptop)
//!
//! A/B on this exact fixture (200 frames each):
//!
//! | | frame p50 | frame p95 |
//! |---|---|---|
//! | pre-change lens (GUI src stashed)        | 21.882ms | 24.854ms |
//! | post-change, empty ranking               | 25.974ms | 28.010ms |
//! | post-change, full rank data (this test)  | ~26ms    | 28.376ms |
//!
//! The rank *data* costs ~0.4ms p95 even at this deliberately absurd
//! density (every task ranked — the feature is sparse by design); the
//! rest (~3.1ms) is the two painted arrow buttons and expedite pill per
//! row, ~9µs/row in a debug build. The dominant lens cost is the
//! pre-existing unvirtualized row rendering (TASK-13), which this change
//! does not touch.
//!
//! Run explicitly:
//! ```sh
//! cargo test -p switchbard-gui --test projects_rank_perf_smoke -- --ignored --nocapture
//! ```

mod common;

use std::fs;
use std::path::PathBuf;

use common::{harness, isolated_config_save_path};
use switchbard_core::config::Config;
use switchbard_core::{
    BacklogRepo, BacklogTask, BacklogTaskSource, Repo, RepoRanking, WorktreeRef,
};
use switchbard_gui::app::HiveApp;
use switchbard_gui::runtime::{BacklogLens, ViewTab};

const REPOS: usize = 4;
const PROJECTS_PER_REPO: usize = 5;
const TASKS_PER_PROJECT: usize = 20;
const FRAMES: usize = 200;

/// A ceiling over the measured ~28ms p95 (see the module doc's A/B), not a
/// target — headroom for a loaded CI box, tight enough that per-frame
/// filesystem work or an accidental O(n²) over the rank lists fails the
/// check.
const FRAME_P95_BUDGET_MS: f64 = 40.0;

fn task(repo: usize, project: usize, i: usize) -> BacklogTask {
    let id = format!("TASK-{repo}{project}{i:02}");
    BacklogTask {
        title: format!("Task {i} of project {project} in repo {repo}"),
        status: "To Do".to_string(),
        priority: "medium".to_string(),
        assignees: vec![],
        labels: vec![],
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
        path: PathBuf::from(format!("/tmp/switchbard-rank-perf/{id}.md")),
        id,
    }
}

fn build_fixture() -> HiveApp {
    let mut repos = Vec::new();
    let mut worktrees = Vec::new();
    let mut backlog_repos = Vec::new();
    for r in 0..REPOS {
        let repo_path = PathBuf::from(format!("/tmp/switchbard-rank-perf/repo-{r}"));
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
        let mut ranking = RepoRanking::default();
        for p in 0..PROJECTS_PER_REPO {
            let name = format!("Project {r}-{p}");
            ranking.projects.push(name.clone());
            let mut ids = Vec::new();
            for i in 0..TASKS_PER_PROJECT {
                let task = task(r, p, i);
                ids.push(task.id.clone());
                if i == 0 {
                    ranking.expedite.push(task.id.clone());
                }
                tasks.push(task);
            }
            ranking.tasks.insert(name, ids);
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
                // `RANK_PERF_BASELINE=1` empties the rank data for the A/B in
                // the module doc (isolates data cost from widget cost).
                ranking: if std::env::var("RANK_PERF_BASELINE").is_ok() {
                    RepoRanking::default()
                } else {
                    ranking
                },
                loaded_at_unix: 0,
                configured_statuses: vec!["To Do".into(), "In Progress".into(), "Done".into()],
            },
        ));
    }

    let mut cfg = Config::default();
    cfg.ui.onboarding_dismissed = true;
    let mut app = HiveApp::new_headless(cfg, repos, worktrees);
    app.config_save_path = Some(isolated_config_save_path());
    app.view_tab = ViewTab::Backlog;
    app.backlog_view.lens = BacklogLens::Projects;
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
fn projects_lens_rank_perf_smoke() {
    let log_path = std::env::temp_dir().join(format!(
        "switchbard-rank-perf-smoke-{}.csv",
        std::process::id()
    ));
    // SAFETY (single-threaded-in-practice): one `#[ignore]`d test in this
    // binary, always run standalone via `-- --ignored` — same env-var
    // contract as `workspace_perf_smoke.rs`.
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
        "Stack-ranking Projects-lens perf smoke — {REPOS} repos x {PROJECTS_PER_REPO} projects \
         x {TASKS_PER_PROJECT} tasks ({} tasks, all ranked, {} expedited), {FRAMES} frames:",
        REPOS * PROJECTS_PER_REPO * TASKS_PER_PROJECT,
        REPOS * PROJECTS_PER_REPO,
    );
    println!(
        "  frame  p50 {:.3}ms  p95 {:.3}ms  max {:.3}ms",
        total.p50, total.p95, total.max
    );

    assert!(
        total.p95 < FRAME_P95_BUDGET_MS,
        "frame p95 {:.3}ms exceeds the {FRAME_P95_BUDGET_MS}ms budget — something \
         per-frame-expensive landed on the Projects lens rank path",
        total.p95
    );
}
