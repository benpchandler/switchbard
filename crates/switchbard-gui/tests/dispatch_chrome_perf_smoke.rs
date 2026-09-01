//! TASK-43 render-path perf smoke (CLAUDE.md's render-path perf rule — the
//! top bar gained a per-frame dispatch summary that feeds the ambient chip and
//! the Dispatches tab badge, so it now runs on *every* frame of *every* tab,
//! including tabs that never look at dispatch).
//!
//! Measures the `top_bar_ms` column specifically, not just total frame time:
//! that column is exactly the surface this task touched, and a regression
//! there would otherwise hide inside the noise of a whole frame.
//!
//! Same shape as `workspace_perf_smoke.rs` (see its module doc for why this
//! drives the real `render_ui` through `egui_kittest` and why it is
//! `#[ignore]`d rather than run on every `cargo test`). The fixture is
//! deliberately hostile to the summary path: every tracked repo carries a
//! full backlog, and a realistic slice of those tasks carries dispatch labels,
//! because `summarize_dispatch` scans every task's labels and does its (small)
//! key clones only for the flagged ones.
//!
//! ## Measured (2026-08-19, debug build, M-series laptop)
//!
//! A/B on this exact fixture, toggling only the `summarize_dispatch` call in
//! `top_bar::render` (baseline substitutes `DispatchSummary::default()`):
//!
//! | | top bar p50 | top bar p95 | frame p95 |
//! |---|---|---|---|
//! | baseline (no summary) | 0.212ms | 0.262ms | 2.817ms |
//! | TASK-43 (summary on)  | 0.270ms | 0.296ms | 2.637ms |
//!
//! ~0.03ms of p95 top-bar time for 440 tasks / 44 dispatch-labeled, in a
//! *debug* build — against a 16.7ms frame budget, and inside the frame-level
//! noise. The cost is a per-frame label scan over the already-cached backlog,
//! which is the shape the design intends; anything that regresses it into
//! filesystem or per-worktree work blows the assertion below by an order of
//! magnitude.
//!
//! (An earlier measurement of this same A/B read 0.367ms p95. The difference
//! is the audit's F6 fix: the summary now folds `dispatch_category`, which
//! decides the label ladder without parsing notes, rather than
//! `dispatch_state`, which allocated a `String` per finished task to extract
//! a PR link the top bar never renders. Correcting the doc's claim about the
//! cost turned out to also remove a third of it.)
//!
//! Run explicitly:
//! ```sh
//! cargo test -p switchbard-gui --test dispatch_chrome_perf_smoke -- --ignored --nocapture
//! ```

mod common;

use std::fs;
use std::path::PathBuf;

use common::{harness, isolated_config_save_path};
use switchbard_core::config::Config;
use switchbard_core::dispatch_inspect::{now_unix, DispatchRun, DispatchRunLiveness};
use switchbard_core::{
    BacklogRepo, BacklogTask, BacklogTaskSource, Repo, WorktreeRef, DISPATCHED_LABEL,
    DISPATCHING_LABEL, DISPATCH_FAILED_LABEL, DISPATCH_LABEL,
};
use switchbard_gui::app::HiveApp;

/// Sized past the real 2026-08-19 machine (8 backlog-bearing repos, ~320
/// tasks total) so the measurement has headroom over the environment it has
/// to hold up in.
const REPOS: usize = 11;
const TASKS_PER_REPO: usize = 40;
/// Every 10th task carries a dispatch label — far more dispatch traffic than
/// the real opt-in feature ever sees at once.
const DISPATCH_EVERY: usize = 10;
const FRAMES: usize = 200;

/// A ceiling, not a target: generous enough not to flake on a loaded CI box,
/// tight enough that anything filesystem-shaped sneaking onto this path
/// (a `read_dir`, a `metadata`, a full snapshot rebuild) fails the check.
const TOP_BAR_P95_BUDGET_MS: f64 = 3.0;

fn label_for(i: usize) -> Option<&'static str> {
    if !i.is_multiple_of(DISPATCH_EVERY) {
        return None;
    }
    Some(match (i / DISPATCH_EVERY) % 4 {
        0 => DISPATCH_LABEL,
        1 => DISPATCHING_LABEL,
        2 => DISPATCH_FAILED_LABEL,
        _ => DISPATCHED_LABEL,
    })
}

fn task(repo: usize, i: usize) -> BacklogTask {
    let id = format!("TASK-{repo}-{i}");
    BacklogTask {
        title: format!("Task {i} in repo {repo}"),
        status: "To Do".to_string(),
        priority: "medium".to_string(),
        assignees: vec!["ben".to_string()],
        labels: label_for(i)
            .map(|l| vec![l.to_string(), "hub".to_string()])
            .unwrap_or_else(|| vec!["hub".to_string()]),
        dependencies: vec![],
        references: vec![],
        project: None,
        parent: None,
        created_date: Some("2026-06-01 09:00".to_string()),
        updated_date: Some("2026-06-01 09:00".to_string()),
        description: "Lorem ipsum dolor sit amet.".to_string(),
        implementation_plan: String::new(),
        // Notes matter: `dispatch_state` scans them for the PR link / failure
        // reason on every dispatch-labeled task it sees.
        implementation_notes:
            "Dispatch PR: https://example.test/pr/1\nDispatch failed: claude exited with Some(1)"
                .to_string(),
        final_summary: String::new(),
        acceptance_criteria: vec![],
        definition_of_done: vec![],
        source: BacklogTaskSource::Active,
        path: PathBuf::from(format!("/tmp/switchbard-dispatch-perf/{id}.md")),
        id,
    }
}

fn build_fixture() -> HiveApp {
    let mut repos = Vec::new();
    let mut worktrees = Vec::new();
    let mut backlog_repos = Vec::new();
    for r in 0..REPOS {
        let repo_path = PathBuf::from(format!("/tmp/switchbard-dispatch-perf/repo-{r}"));
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
        backlog_repos.push((
            repo_path.clone(),
            BacklogRepo {
                root: repo_path,
                tasks: (0..TASKS_PER_REPO).map(|i| task(r, i)).collect(),
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
            },
        ));
    }

    let mut cfg = Config::default();
    cfg.ui.onboarding_dismissed = true;
    let mut app = HiveApp::new_headless(cfg, repos, worktrees);
    app.config_save_path = Some(isolated_config_save_path());

    {
        let mut cached_projects = app.backlog_repos.lock().unwrap();
        let mut cached_runs = app.dispatch_runs.lock().unwrap();
        let now = now_unix();
        for (root, repo) in backlog_repos {
            for task in &repo.tasks {
                if label_for_task(task).is_none() {
                    continue;
                }
                cached_runs.insert(
                    (root.clone(), task.id.clone()),
                    DispatchRun {
                        task_id: task.id.clone(),
                        branch: format!("dispatch/{}", task.id.to_lowercase()),
                        worktree_path: root.join(".worktrees").join(&task.id),
                        worktree_exists: false,
                        log_path: Some(PathBuf::from("/tmp/switchbard-logs/x.log")),
                        prompt_path: None,
                        started_at_unix: Some(now.saturating_sub(300)),
                        log_bytes: 0,
                        log_modified_unix: None,
                        // Verified-alive: the state that makes the summary do
                        // the most work per row (a supervision check and an
                        // elapsed-time fold, not an early bail).
                        progress: switchbard_core::dispatch_inspect::RunProgress::default(),
                        liveness: DispatchRunLiveness::Alive {
                            pgid: 4242,
                            supervised: true,
                        },
                    },
                );
            }
            cached_projects.insert(root, repo);
        }
    }

    app
}

fn label_for_task(task: &BacklogTask) -> Option<&String> {
    task.labels.iter().find(|l| {
        [
            DISPATCH_LABEL,
            DISPATCHING_LABEL,
            DISPATCH_FAILED_LABEL,
            DISPATCHED_LABEL,
        ]
        .contains(&l.as_str())
    })
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
fn top_bar_dispatch_summary_perf_smoke() {
    let log_path = std::env::temp_dir().join(format!(
        "switchbard-dispatch-perf-smoke-{}.csv",
        std::process::id()
    ));
    // SAFETY (single-threaded-in-practice): this file has exactly one
    // `#[test]` and it is `#[ignore]`d — always run standalone via
    // `-- --ignored`, never interleaved with another test in this binary that
    // would race these process-wide env vars. Same contract as
    // `workspace_perf_smoke.rs`.
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
    let mut top_bar_ms = Vec::new();
    for line in csv.lines().skip(1) {
        let cols: Vec<&str> = line.split(',').collect();
        let Some(total) = cols.get(1).and_then(|s| s.parse::<f64>().ok()) else {
            continue;
        };
        let Some(top_bar) = cols.get(2).and_then(|s| s.parse::<f64>().ok()) else {
            continue;
        };
        total_ms.push(total);
        top_bar_ms.push(top_bar);
    }
    assert!(
        !total_ms.is_empty(),
        "perf log at {} recorded no frames",
        log_path.display()
    );

    let total = percentiles(total_ms);
    let top_bar = percentiles(top_bar_ms);
    println!(
        "TASK-43 dispatch-chrome perf smoke — {REPOS} repos x {TASKS_PER_REPO} tasks \
         ({} total, {} dispatch-labeled), {FRAMES} frames:",
        REPOS * TASKS_PER_REPO,
        REPOS * TASKS_PER_REPO / DISPATCH_EVERY,
    );
    println!(
        "  frame    p50 {:.3}ms  p95 {:.3}ms  max {:.3}ms",
        total.p50, total.p95, total.max
    );
    println!(
        "  top bar  p50 {:.3}ms  p95 {:.3}ms  max {:.3}ms",
        top_bar.p50, top_bar.p95, top_bar.max
    );

    assert!(
        top_bar.p95 < TOP_BAR_P95_BUDGET_MS,
        "top bar p95 {:.3}ms exceeds the {TOP_BAR_P95_BUDGET_MS}ms budget — \
         something filesystem-shaped or O(worktrees) got onto the chip's path",
        top_bar.p95
    );

    let _ = fs::remove_file(&log_path);
}
