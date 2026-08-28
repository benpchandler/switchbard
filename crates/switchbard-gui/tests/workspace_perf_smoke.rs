//! TASK-41 render-path perf smoke (CLAUDE.md's render-path perf rule — the
//! Workspace view's row rendering was touched: staleness badge, size label,
//! and bulk-select checkbox, once per worktree row, plus a once-per-frame
//! filter-chip count pass).
//!
//! `HiveApp::new_headless` reads `SWITCHBARD_PERF`/`SWITCHBARD_PERF_LOG` at
//! construction time (`PerfSession::from_env`) exactly like the packaged app
//! does, so this drives the *real* `render_ui` path (not a synthetic stand-in)
//! through `egui_kittest`'s headless (no GPU, no real window) harness —
//! deterministic, CI-safe, and fast enough to actually run, unlike the
//! wgpu-backed snapshot tests (`ui_snapshot.rs`, `qa_screenshots.rs`), which
//! this mirrors in being `#[ignore]`d (a perf smoke should be run
//! deliberately, not on every `cargo test`).
//!
//! No pre-TASK-41 baseline build was captured for a strict before/after diff
//! (the render path didn't exist to measure before this task); this reports
//! absolute p50/p95/max frame + workspace time against a machine sized close
//! to the real ~138-worktree environment cited in TASK-41, with a loose
//! upper-bound assertion so a severe regression still fails the check.
//!
//! Run explicitly:
//! ```sh
//! cargo test -p switchbard-gui --test workspace_perf_smoke -- --ignored --nocapture
//! ```

mod common;

use std::fs;
use std::path::PathBuf;
use std::time::Instant;

use common::{harness, isolated_config_save_path};
use switchbard_core::config::Config;
use switchbard_core::{DriftProbe, Repo, WorktreeRef, WorktreeStaleness};
use switchbard_gui::app::HiveApp;
use switchbard_gui::runtime::{WorktreeMeta, WorktreeSizeEntry};

/// `repos * (1 primary + per_repo linked)` worktrees, close to the real
/// 2026-08-19 machine's ~138 across 11 repos this task's brief cites.
const REPOS: usize = 11;
const LINKED_PER_REPO: usize = 12;
const FRAMES: usize = 200;

fn build_fixture() -> HiveApp {
    let mut repos = Vec::new();
    let mut worktrees = Vec::new();
    for r in 0..REPOS {
        let repo_path = PathBuf::from(format!("/tmp/switchbard-perf-smoke/repo-{r}"));
        let repo_name = format!("repo-{r}");
        repos.push(Repo {
            name: repo_name.clone(),
            path: repo_path.clone(),
        });
        worktrees.push(WorktreeRef {
            repo_name: repo_name.clone(),
            path: repo_path.clone(),
            branch: Some("main".to_string()),
            head: "aaaa1111".to_string(),
        });
        for w in 0..LINKED_PER_REPO {
            worktrees.push(WorktreeRef {
                repo_name: repo_name.clone(),
                path: PathBuf::from(format!("/tmp/switchbard-perf-smoke/repo-{r}-wt-{w}")),
                branch: Some(format!("feat/wt-{w}")),
                head: "bbbb2222".to_string(),
            });
        }
    }

    let mut cfg = Config::default();
    cfg.ui.onboarding_dismissed = true;
    let mut app = HiveApp::new_headless(cfg, repos, worktrees.clone());
    app.config_save_path = Some(isolated_config_save_path());

    // Seed meta + size for every worktree so rows hit the real Merged/
    // Orphan/Live/size render branches instead of the cheap "pending" path —
    // a perf smoke that measures the "..." placeholder isn't measuring the
    // feature.
    {
        let mut meta = app.meta.lock().unwrap();
        let mut sizes = app.sizes.lock().unwrap();
        for (i, w) in worktrees.iter().enumerate() {
            let staleness = match i % 3 {
                0 => WorktreeStaleness::Merged {
                    base: "main".to_string(),
                    evidence: switchbard_core::LandedEvidence::Ancestry,
                },
                1 => WorktreeStaleness::NoUpstream,
                _ => WorktreeStaleness::Live,
            };
            let dirty = i % 5 == 0;
            meta.insert(
                w.path.clone(),
                WorktreeMeta {
                    dirty_files: Some(if dirty {
                        vec![" M src/lib.rs".to_string()]
                    } else {
                        vec![]
                    }),
                    trunk: Some(switchbard_core::TrunkDivergence {
                        base: "origin/main".to_string(),
                        unlanded: (i % 4) as u32,
                        ancestry_ahead: (i % 4) as u32,
                        behind: 0,
                    }),
                    remote_drift: Some(DriftProbe::Ready {
                        base: "origin/main".to_string(),
                        ahead: 0,
                        behind: (i % 3) as u32,
                    }),
                    staleness: Some(staleness),
                    ..Default::default()
                },
            );
            sizes.insert(
                w.path.clone(),
                WorktreeSizeEntry {
                    bytes: Some(200_000_000 + i as u64 * 37),
                    computed_at: Instant::now(),
                },
            );
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
fn workspace_render_perf_smoke_with_140ish_worktrees() {
    let log_path =
        std::env::temp_dir().join(format!("switchbard-perf-smoke-{}.csv", std::process::id()));
    // SAFETY (single-threaded-in-practice): this test file has exactly one
    // `#[test]`, and it's `#[ignore]`d — always run standalone via `--
    // --ignored`, never interleaved with another test in this binary that
    // would race these process-wide env vars.
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
    let mut workspace_ms = Vec::new();
    for line in csv.lines().skip(1) {
        let cols: Vec<&str> = line.split(',').collect();
        let Some(total) = cols.get(1).and_then(|s| s.parse::<f64>().ok()) else {
            continue;
        };
        let Some(workspace) = cols.get(5).and_then(|s| s.parse::<f64>().ok()) else {
            continue;
        };
        total_ms.push(total);
        workspace_ms.push(workspace);
    }
    assert!(
        !total_ms.is_empty(),
        "perf log at {} recorded no frames",
        log_path.display()
    );

    let total = percentiles(total_ms);
    let workspace = percentiles(workspace_ms);
    println!(
        "TASK-41 workspace perf smoke — {} repos x {} linked worktrees ({} total), {} frames:",
        REPOS,
        LINKED_PER_REPO,
        REPOS * (LINKED_PER_REPO + 1),
        FRAMES
    );
    println!(
        "  frame     p50 {:.2}ms  p95 {:.2}ms  max {:.2}ms",
        total.p50, total.p95, total.max
    );
    println!(
        "  workspace p50 {:.2}ms  p95 {:.2}ms  max {:.2}ms",
        workspace.p50, workspace.p95, workspace.max
    );

    // Loose upper bound — this is a regression tripwire, not a tight budget:
    // headless kittest render on a dev machine over ~140 rows should stay
    // comfortably under one frame's worth of a 30fps budget (33ms) even with
    // every row hitting the new badge/size/checkbox render paths.
    assert!(
        total.p95 < 33.0,
        "workspace render p95 regressed past a single 30fps frame budget: {:.2}ms",
        total.p95
    );
}
