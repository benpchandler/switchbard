//! Read-only wall-time + subprocess-count audit for each periodic worker's
//! per-tick cost, run against this machine's real `~/.switchbard/
//! config.toml` repos. Written for the scan-cadence tuning mission (owner
//! report: "we may be scanning a lot of directories and files
//! simultaneously" on a machine with ~80+ worktrees across a handful of
//! repos). Never mutates anything — no config write, no git state change,
//! no cache write; only reads.
//!
//! Usage: cargo run --release --example scan_cadence_audit

use std::time::Instant;
use switchbard_core::{
    config, detect_services, enumerate_worktrees, is_backlog_repo, load_backlog_repo,
    probe_dirty_files, probe_fetch_age, probe_head_commit_time, probe_ignored_files,
    probe_recent_commits, probe_ref_drift_detail, probe_remote_drift, probe_trunk_detail,
    probe_trunk_divergence, probe_worktree_size, probe_worktree_staleness, scan_agent_context,
    scan_listeners, staleness_from_trunk, DriftProbe, Repo, WorktreeRef,
};

/// `du` measured ~0.8-1.5s per real worktree in the 2026-08-19 audit (see
/// `workers.rs`'s cadence-policy doc) — running it over every worktree on a
/// 100+ worktree machine would make this "read-only, never blocks anything"
/// instrument itself a multi-minute blocker. Bounded and logged rather than
/// silently truncated (see the printed note below).
const SIZE_SAMPLE_LIMIT: usize = 20;

fn detect_services_timed(path: &std::path::Path) -> usize {
    detect_services(path).len()
}

/// Same expansion `workers::spawn_probe` and `main.rs` use — reimplemented
/// here (rather than depending on `switchbard_gui`'s private `runtime`
/// module) to keep this example a pure `switchbard-core` consumer, matching
/// every other example in this directory (`probe.rs`, `sweep.rs`).
fn expand(repos: &[Repo]) -> Vec<WorktreeRef> {
    let mut out = Vec::new();
    for repo in repos {
        let mut added_primary = false;
        if let Ok(entries) = enumerate_worktrees(&repo.path) {
            for e in entries {
                if !e.path.exists() {
                    continue;
                }
                if e.path == repo.path {
                    added_primary = true;
                }
                out.push(WorktreeRef {
                    repo_name: repo.name.clone(),
                    path: e.path,
                    branch: e.branch,
                    head: e.head,
                });
            }
        }
        if !added_primary {
            out.push(WorktreeRef {
                repo_name: repo.name.clone(),
                path: repo.path.clone(),
                branch: None,
                head: String::new(),
            });
        }
    }
    out
}

fn main() {
    let cfg = config::load();
    println!("Repos tracked: {}", cfg.repos.len());
    for r in &cfg.repos {
        println!("  {} -> {}", r.name, r.path.display());
    }

    // ---- scanner tick (SCAN_PERIOD = 3s) ----
    let t0 = Instant::now();
    let listeners = scan_listeners();
    let scanner_elapsed = t0.elapsed();
    println!(
        "\n[scanner] lsof snapshot: {:?}, {} listeners (1 subprocess, independent of worktree count)",
        scanner_elapsed,
        listeners.map(|l| l.len()).unwrap_or(0)
    );

    // ---- git-probe tick step 1: re-enumerate worktrees (PROBE_PERIOD = 60s) ----
    let t0 = Instant::now();
    let worktrees = expand(&cfg.repos);
    let enumerate_elapsed = t0.elapsed();
    println!(
        "\n[git-probe step1] enumerate_worktrees x{} repos: {:?} ({} subprocess calls — one `git worktree list` per repo)",
        cfg.repos.len(),
        enumerate_elapsed,
        cfg.repos.len()
    );
    println!("Total worktrees: {}", worktrees.len());

    // ---- git-probe tick step 2: full per-worktree probe suite ----
    let mut subprocess_count = 0usize;
    let t0 = Instant::now();
    let repo_paths_for_trunk: std::collections::HashMap<String, std::path::PathBuf> = cfg
        .repos
        .iter()
        .map(|r| (r.name.clone(), r.path.clone()))
        .collect();
    for w in &worktrees {
        // `probe_trunk_divergence` replaced `probe_main_drift`: same shape,
        // but it resolves the trunk via `default_branch` and counts by
        // patch-equivalence, so it is 3 subprocesses (base lookup + two
        // rev-lists) rather than 2.
        let trunk = repo_paths_for_trunk
            .get(&w.repo_name)
            .and_then(|repo| probe_trunk_divergence(repo, &w.path));
        subprocess_count += if trunk.is_some() { 3 } else { 1 };

        let remote_drift = probe_remote_drift(&w.path);
        subprocess_count += match &remote_drift {
            Some(DriftProbe::NoUpstream) => 1,
            Some(_) => 3,
            None => 1,
        };

        if let Some(d) = &trunk {
            if d.unlanded + d.behind > 0 {
                let _ = probe_trunk_detail(&w.path, d, 5);
                subprocess_count += 3;
            }
        }
        if let Some(d) = &remote_drift {
            if d.is_drifted() {
                let _ = probe_ref_drift_detail(&w.path, d.base().unwrap_or(""), 5);
                subprocess_count += 2;
            }
        }

        let _ = probe_dirty_files(&w.path);
        subprocess_count += 1;
        let _ = probe_ignored_files(&w.path);
        subprocess_count += 1;
        let _ = probe_head_commit_time(&w.path);
        subprocess_count += 1;
        let _ = probe_fetch_age(&w.path);
        subprocess_count += 1;
        let _ = probe_recent_commits(&w.path, 10);
        subprocess_count += 1;
        // Free: the Merged/Orphan/Live badge is derived from the trunk
        // comparison already made above rather than re-probed. It used to be
        // its own ~2-3 subprocess call per worktree — see the [staleness]
        // section below, which still times the standalone probe so the saving
        // stays visible.
        let _ = staleness_from_trunk(trunk.as_ref(), remote_drift.as_ref());
    }
    let probe_elapsed = t0.elapsed();
    println!(
        "\n[git-probe step2] full per-worktree probe over {} worktrees: {:?}, ~{} git subprocess spawns",
        worktrees.len(),
        probe_elapsed,
        subprocess_count
    );

    // ---- per-probe-function breakdown, isolated ----
    macro_rules! time_probe {
        ($label:expr, $call:expr) => {{
            let t0 = Instant::now();
            for w in &worktrees {
                let _ = $call(w);
            }
            println!("    {:<22} {:?}", $label, t0.elapsed());
        }};
    }
    println!(
        "\n[git-probe breakdown] each isolated over all {} worktrees:",
        worktrees.len()
    );
    time_probe!("probe_trunk_divergence", |w: &WorktreeRef| {
        repo_paths_for_trunk
            .get(&w.repo_name)
            .and_then(|repo| probe_trunk_divergence(repo, &w.path))
    });
    time_probe!("probe_trunk_detail", |w: &WorktreeRef| {
        repo_paths_for_trunk
            .get(&w.repo_name)
            .and_then(|repo| probe_trunk_divergence(repo, &w.path))
            .and_then(|d| probe_trunk_detail(&w.path, &d, 5))
    });
    time_probe!("probe_remote_drift", |w: &WorktreeRef| probe_remote_drift(
        &w.path
    ));
    time_probe!("probe_dirty_files", |w: &WorktreeRef| probe_dirty_files(
        &w.path
    ));
    time_probe!(
        "probe_ignored_files",
        |w: &WorktreeRef| probe_ignored_files(&w.path)
    );
    time_probe!("probe_head_commit_time", |w: &WorktreeRef| {
        probe_head_commit_time(&w.path)
    });
    time_probe!("probe_fetch_age", |w: &WorktreeRef| probe_fetch_age(
        &w.path
    ));
    time_probe!("probe_recent_commits(10)", |w: &WorktreeRef| {
        probe_recent_commits(&w.path, 10)
    });
    println!(
        "  -> average per worktree: {:?}",
        probe_elapsed / worktrees.len().max(1) as u32
    );
    println!(
        "  -> combined git-probe tick (enumerate + probe): {:?}",
        enumerate_elapsed + probe_elapsed
    );

    // ---- detection tick (DETECT_PERIOD = 30s): cold pass, every worktree unseen ----
    let t0 = Instant::now();
    for w in &worktrees {
        let _ = detect_services_timed(&w.path);
    }
    let detect_elapsed = t0.elapsed();
    println!(
        "\n[detection] cold detect_services over {} worktrees: {:?} (0 subprocesses; steady-state cost is ~0 once cached — idempotent per worktree)",
        worktrees.len(),
        detect_elapsed
    );

    // ---- agent-context tick (CONTEXT_PERIOD = 30s): cold scan, every worktree unseen ----
    let t0 = Instant::now();
    let mut item_total = 0usize;
    for w in &worktrees {
        let map = scan_agent_context(&w.path);
        item_total += map.items.len();
    }
    let context_elapsed = t0.elapsed();
    println!(
        "\n[agent-context] cold scan_agent_context over {} worktrees: {:?}, {} items found (0 subprocesses; recursive fs walk per worktree, skips .git/node_modules/target/dist/build/.next/.nuxt/vendor)",
        worktrees.len(),
        context_elapsed,
        item_total
    );
    println!(
        "  -> average per worktree: {:?}",
        context_elapsed / worktrees.len().max(1) as u32
    );

    // ---- backlog tick (BACKLOG_PERIOD = 30s): one root per repo, not per worktree ----
    let roots: Vec<_> = {
        let mut seen = std::collections::HashSet::new();
        cfg.repos
            .iter()
            .map(|r| r.path.clone())
            .filter(|p| seen.insert(p.clone()))
            .collect()
    };
    let t0 = Instant::now();
    let mut task_total = 0usize;
    for root in &roots {
        if !is_backlog_repo(root) {
            continue;
        }
        if let Ok(repo) = load_backlog_repo(root) {
            task_total += repo.tasks.len();
        }
    }
    let backlog_elapsed = t0.elapsed();
    println!(
        "\n[backlog] load_backlog_repo over {} repo roots (NOT per-worktree): {:?}, {} tasks total (0 subprocesses; markdown file reads)",
        roots.len(),
        backlog_elapsed,
        task_total
    );

    // ---- TASK-41 staleness probe: same cost class as the other git probes ----
    let repo_paths: std::collections::HashMap<String, std::path::PathBuf> = cfg
        .repos
        .iter()
        .map(|r| (r.name.clone(), r.path.clone()))
        .collect();
    let (mut merged, mut no_upstream, mut live, mut unclassified) =
        (0usize, 0usize, 0usize, 0usize);
    let t0 = Instant::now();
    for w in &worktrees {
        match repo_paths
            .get(&w.repo_name)
            .map(|repo_path| probe_worktree_staleness(repo_path, &w.path))
        {
            Some(switchbard_core::WorktreeStaleness::Merged { .. }) => merged += 1,
            Some(switchbard_core::WorktreeStaleness::NoUpstream) => no_upstream += 1,
            Some(switchbard_core::WorktreeStaleness::Live) => live += 1,
            // `Unknown` is git declining to answer; `None` is the repo not
            // being tracked. Neither is a classification, so both land here.
            Some(switchbard_core::WorktreeStaleness::Unknown) | None => unclassified += 1,
        }
    }
    let staleness_elapsed = t0.elapsed();
    println!(
        "\n[staleness] standalone probe_worktree_staleness over {} worktrees: {:?} (~2-3 git subprocess spawns each). NOT part of the tick any more: `spawn_probe` derives the badge from the trunk comparison via `staleness_from_trunk`, so this is the cost that derivation removes.",
        worktrees.len(),
        staleness_elapsed
    );
    println!(
        "  -> merged {merged}, no-upstream {no_upstream}, live {live}, unclassified (no repo path found) {unclassified}"
    );

    // ---- TASK-41 size probe (`du`): the outlier — see SIZE_SAMPLE_LIMIT's doc ----
    let sample: Vec<&WorktreeRef> = worktrees.iter().take(SIZE_SAMPLE_LIMIT).collect();
    let t0 = Instant::now();
    for w in &sample {
        let _ = probe_worktree_size(&w.path);
    }
    let size_elapsed = t0.elapsed();
    println!(
        "\n[size] probe_worktree_size (`du -sk`) over a bounded sample of {}/{} worktrees: {:?}",
        sample.len(),
        worktrees.len(),
        size_elapsed
    );
    if !sample.is_empty() {
        println!(
            "  -> average per worktree: {:?} (see workers::SIZE_MAX_MISSING_PER_TICK / SIZE_PERIOD for why this never runs as one unbounded sweep)",
            size_elapsed / sample.len() as u32
        );
    }
    if worktrees.len() > sample.len() {
        println!(
            "  -> {} worktrees NOT sampled (SIZE_SAMPLE_LIMIT={}) — re-run with a higher limit for a full-machine number, deliberately, not by default",
            worktrees.len() - sample.len(),
            SIZE_SAMPLE_LIMIT
        );
    }
}
