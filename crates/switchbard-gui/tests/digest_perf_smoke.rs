//! TASK-99 render-path perf smoke (CLAUDE.md's render-path perf rule —
//! `ui::places::digest` is the new `Place::Digest` body, and Digest is the
//! app's landing place, so this render path runs on *every* frame the app
//! opens onto by default).
//!
//! Measures `central_ms` specifically — the column covering exactly
//! `ui::places::digest::render` when `Place::Digest` is active (see
//! `HiveApp::render_ui`'s `perf.record_central` call and the CSV header in
//! `perf.rs`) — not just total frame time, so a regression here does not
//! hide inside the top bar / nav noise `dispatch_chrome_perf_smoke.rs`
//! already accounts for.
//!
//! Same shape as `workspace_perf_smoke.rs`/`dispatch_chrome_perf_smoke.rs`
//! (see their module docs for why this drives the real `render_ui` through
//! `egui_kittest` and why it is `#[ignore]`d). The fixture is deliberately
//! hostile to every collection this render path walks per frame: every
//! tracked repo carries a full backlog with a realistic mix of in-flight,
//! failed, and stalled dispatch work, plus a pile of unattributed listeners
//! and retired worktrees so all three "Needs a human" row types are
//! non-empty at once.
//!
//! ## Measured (2026-09-01, debug build, M-series laptop)
//!
//! 11 repos x 40 tasks (440 total, 22 dispatch-labeled: failed / stalled /
//! healthy round-robin), 12 unattributed listeners, 11 retired worktrees —
//! an already-bad day's worth of attention-feed rows, not the common case
//! (a healthy Digest renders a handful of rows or none):
//!
//! | | frame p50 | frame p95 | central p50 | central p95 |
//! |---|---|---|---|---|
//! | this fixture | ~4.1ms | ~5–13ms (noisy) | ~3.1ms | ~3.2–6.3ms |
//!
//! `collect_task_rows` locks `backlog_repos`/`dispatch_runs` directly rather
//! than cloning the whole cache (`HiveApp::backlog_repos_snapshot()` deep-
//! clones every task's full body — description, plan, notes) — an initial
//! version of this file caught that costing ~9ms of *central* p95 alone at
//! 4x this fixture's dispatch-row density, before any actual widget paints.
//! The goal-cards call (`ui::backlog::digest::render_goal_cards_for_digest_
//! place`, reused rather than forked per this task's own mandate) still
//! pays that clone once per frame — untouched, shared machinery, not this
//! task's to optimize.
//!
//! Run explicitly:
//! ```sh
//! cargo test -p switchbard-gui --test digest_perf_smoke -- --ignored --nocapture
//! ```

mod common;

use std::fs;
use std::path::PathBuf;

use common::{harness, isolated_config_save_path};
use switchbard_core::config::Config;
use switchbard_core::dispatch_inspect::{now_unix, DispatchRun, DispatchRunLiveness};
use switchbard_core::{
    AttributedListener, BacklogRepo, BacklogTask, BacklogTaskSource, Fact, LandedEvidence,
    LocalListener, Repo, WorktreeRef, WorktreeStaleness, DISPATCHING_LABEL, DISPATCH_FAILED_LABEL,
};
use switchbard_gui::app::HiveApp;
use switchbard_gui::runtime::{Place, WorktreeMeta};

/// Sized past the real 2026-08-19 dispatch-perf-smoke machine (8 backlog
/// repos, ~320 tasks) so this has headroom over the environment it has to
/// hold up in.
const REPOS: usize = 11;
const TASKS_PER_REPO: usize = 40;
/// Every 8th task is dispatch-labeled (failed / stalled / healthy in-flight,
/// round-robin) — far more dispatch traffic than the opt-in feature sees at
/// once in practice, on purpose.
const DISPATCH_EVERY: usize = 20;
/// Extra non-repo-owned worktrees per repo, all evaluated `Safe` (worst case
/// for `collect_worktree_rows`: every one of them actually renders a row).
const LINKED_PER_REPO: usize = 1;
const UNATTRIBUTED_LISTENERS: usize = 12;
const FRAMES: usize = 200;

/// A ceiling, not a target: generous enough not to flake on a loaded CI box,
/// tight enough that anything filesystem-shaped sneaking onto this path (a
/// `read_dir`, a `git` shell-out, a full snapshot rebuild) fails the check.
const CENTRAL_P95_BUDGET_MS: f64 = 8.0;

fn task(repo: usize, i: usize) -> BacklogTask {
    let id = format!("TASK-{repo}-{i}");
    let (status, labels, notes) = if i.is_multiple_of(DISPATCH_EVERY) {
        match (i / DISPATCH_EVERY) % 3 {
            0 => (
                "To Do",
                vec![DISPATCH_FAILED_LABEL.to_string()],
                "Dispatch failed: claude exited with Some(1)".to_string(),
            ),
            // Stalled: the run fixture below gives this one an age past
            // `stale_after`, so the label alone doesn't decide it.
            1 => ("To Do", vec![DISPATCHING_LABEL.to_string()], String::new()),
            _ => ("In Progress", vec![], String::new()),
        }
    } else {
        ("To Do", vec![], String::new())
    };
    BacklogTask {
        title: format!("Task {i} in repo {repo}"),
        status: status.to_string(),
        priority: "medium".to_string(),
        assignees: vec!["ben".to_string()],
        labels,
        dependencies: vec![],
        references: vec![],
        project: None,
        parent: None,
        created_date: Some("2026-06-01 09:00".to_string()),
        updated_date: Some("2026-06-01 09:00".to_string()),
        description: "Lorem ipsum dolor sit amet.".to_string(),
        implementation_plan: String::new(),
        implementation_notes: notes,
        final_summary: String::new(),
        acceptance_criteria: vec![],
        definition_of_done: vec![],
        source: BacklogTaskSource::Active,
        path: PathBuf::from(format!("/tmp/switchbard-digest-perf/{id}.md")),
        id,
    }
}

fn build_fixture() -> HiveApp {
    let mut repos = Vec::new();
    let mut worktrees = Vec::new();
    let mut backlog_repos = Vec::new();
    for r in 0..REPOS {
        let repo_path = PathBuf::from(format!("/tmp/switchbard-digest-perf/repo-{r}"));
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
                path: PathBuf::from(format!("/tmp/switchbard-digest-perf/repo-{r}-wt-{w}")),
                branch: Some(format!("feat/retired-{w}")),
                head: "bbbb2222".to_string(),
            });
        }
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
    let mut app = HiveApp::new_headless(cfg, repos, worktrees.clone());
    app.config_save_path = Some(isolated_config_save_path());
    app.place = Place::Digest;

    {
        let mut cached_projects = app.backlog_repos.lock().unwrap();
        let mut cached_runs = app.dispatch_runs.lock().unwrap();
        let now = now_unix();
        for (root, repo) in backlog_repos {
            for t in &repo.tasks {
                if t.labels.contains(&DISPATCHING_LABEL.to_string()) {
                    cached_runs.insert(
                        (root.clone(), t.id.clone()),
                        DispatchRun {
                            task_id: t.id.clone(),
                            branch: format!("dispatch/{}", t.id.to_lowercase()),
                            worktree_path: root.join(".worktrees").join(&t.id),
                            worktree_exists: false,
                            log_path: Some(PathBuf::from("/tmp/switchbard-logs/x.log")),
                            prompt_path: None,
                            // Past the default 30-minute `stale_after` —
                            // exercises the stalled branch, the most
                            // expensive one (`looks_stalled` +
                            // `killable_pgid`), on every such task.
                            started_at_unix: Some(now.saturating_sub(2_400)),
                            log_bytes: 0,
                            log_modified_unix: None,
                            progress: switchbard_core::dispatch_inspect::RunProgress::default(),
                            liveness: DispatchRunLiveness::Alive {
                                pgid: 4242,
                                supervised: true,
                            },
                        },
                    );
                }
            }
            cached_projects.insert(root, repo);
        }
    }

    {
        let mut meta = app.meta.lock().unwrap();
        for w in &worktrees {
            if w.branch
                .as_deref()
                .is_some_and(|b| b.starts_with("feat/retired"))
            {
                meta.insert(
                    w.path.clone(),
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
            }
        }
    }

    {
        let mut state = app.state.lock().unwrap();
        for i in 0..UNATTRIBUTED_LISTENERS {
            state.listeners.push(AttributedListener {
                listener: LocalListener {
                    pid: 1000 + i as u32,
                    pgid: 2000 + i as i32,
                    port: (5000 + i) as u16,
                    command_name: format!("rogue-{i}"),
                    cwd: None,
                },
                repo_name: None,
                worktree_path: None,
                worktree_branch: None,
            });
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
fn digest_place_render_perf_smoke() {
    let log_path = std::env::temp_dir().join(format!(
        "switchbard-digest-perf-smoke-{}.csv",
        std::process::id()
    ));
    // SAFETY (single-threaded-in-practice): this file has exactly one
    // `#[test]` and it is `#[ignore]`d — always run standalone via
    // `-- --ignored`, never interleaved with another test in this binary
    // that would race these process-wide env vars. Same contract as
    // `dispatch_chrome_perf_smoke.rs`.
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
    let mut central_ms = Vec::new();
    for line in csv.lines().skip(1) {
        let cols: Vec<&str> = line.split(',').collect();
        let Some(total) = cols.get(1).and_then(|s| s.parse::<f64>().ok()) else {
            continue;
        };
        let Some(central) = cols.get(5).and_then(|s| s.parse::<f64>().ok()) else {
            continue;
        };
        total_ms.push(total);
        central_ms.push(central);
    }
    assert!(
        !total_ms.is_empty(),
        "perf log at {} recorded no frames",
        log_path.display()
    );

    let total = percentiles(total_ms);
    let central = percentiles(central_ms);
    println!(
        "TASK-99 digest-place perf smoke — {REPOS} repos x {TASKS_PER_REPO} tasks \
         ({} total, {} dispatch-labeled, {} unattributed listeners, {} retired worktrees), \
         {FRAMES} frames:",
        REPOS * TASKS_PER_REPO,
        REPOS * TASKS_PER_REPO / DISPATCH_EVERY,
        UNATTRIBUTED_LISTENERS,
        REPOS * LINKED_PER_REPO,
    );
    println!(
        "  frame    p50 {:.3}ms  p95 {:.3}ms  max {:.3}ms",
        total.p50, total.p95, total.max
    );
    println!(
        "  central  p50 {:.3}ms  p95 {:.3}ms  max {:.3}ms",
        central.p50, central.p95, central.max
    );

    assert!(
        central.p95 < CENTRAL_P95_BUDGET_MS,
        "Digest place's central-panel p95 ({:.3}ms) exceeded the {:.1}ms budget — \
         something on this path likely regressed into per-frame I/O",
        central.p95,
        CENTRAL_P95_BUDGET_MS
    );

    let _ = fs::remove_file(&log_path);
}
