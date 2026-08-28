//! Render-path smoke for the Agents / Hooks surface under a many-hook load.
//!
//! Run explicitly:
//! cargo test -p switchbard-gui --test agent_hooks_perf_smoke -- --ignored --nocapture

mod common;

use std::fs;
use std::path::PathBuf;

use common::{harness, isolated_config_save_path};
use switchbard_core::config::Config;
use switchbard_core::{AgentContextMap, AgentHook, AgentKind, ContextScope, Repo, WorktreeRef};
use switchbard_gui::app::HiveApp;
use switchbard_gui::runtime::{AgentsSection, ViewTab};

const REPOS: usize = 8;
const HOOKS_PER_REPO: usize = 30;
const FRAMES: usize = 200;

fn build_fixture() -> HiveApp {
    let mut repos = Vec::with_capacity(REPOS);
    let mut worktrees = Vec::with_capacity(REPOS);
    for index in 0..REPOS {
        let path = PathBuf::from(format!("/tmp/switchbard-agent-hooks-perf/repo-{index}"));
        let name = format!("repo-{index}");
        repos.push(Repo {
            name: name.clone(),
            path: path.clone(),
        });
        worktrees.push(WorktreeRef {
            repo_name: name,
            path,
            branch: Some("main".to_string()),
            head: "abc1234".to_string(),
        });
    }
    let mut config = Config::default();
    config.ui.onboarding_dismissed = true;
    let mut app = HiveApp::new_headless(config, repos, worktrees.clone());
    app.config_save_path = Some(isolated_config_save_path());
    app.view_tab = ViewTab::Agents;
    app.agent_context_view.section = AgentsSection::Hooks;
    let maps = worktrees
        .iter()
        .map(|worktree| (worktree.path.clone(), hook_map(worktree)))
        .collect();
    *app.agent_contexts
        .lock()
        .expect("invariant: agent context cache lock") = maps;
    app
}

fn hook_map(worktree: &WorktreeRef) -> AgentContextMap {
    let hooks = (0..HOOKS_PER_REPO)
        .map(|index| AgentHook {
            id: format!("{}#PostToolUse:{index}", worktree.path.display()),
            agent: AgentKind::Claude,
            scope: ContextScope::Local,
            source_path: worktree.path.join(".claude/settings.json"),
            event: "PostToolUse".to_string(),
            matcher: Some("Write|Edit".to_string()),
            hook_type: "command".to_string(),
            action: format!("./scripts/check-{index}.sh"),
            arguments: vec![
                "--strict".to_string(),
                "a-realistically-long-unbroken-argument-for-layout-stress".to_string(),
            ],
            condition: Some("Edit(**/*.rs)".to_string()),
            asynchronous: index % 3 == 0,
            timeout_seconds: Some(30),
        })
        .collect();
    AgentContextMap {
        worktree: worktree.path.clone(),
        hooks,
        ..AgentContextMap::default()
    }
}

#[test]
#[ignore = "perf smoke - run explicitly, see module doc"]
fn agents_hooks_render_stays_within_frame_budget() {
    let log_path = std::env::temp_dir().join(format!(
        "switchbard-agent-hooks-perf-smoke-{}.csv",
        std::process::id()
    ));
    // SAFETY: this ignored test is the only test in this binary.
    unsafe {
        std::env::set_var("SWITCHBARD_PERF", "1");
        std::env::set_var("SWITCHBARD_PERF_LOG", &log_path);
    }
    let mut harness = harness(build_fixture());
    for _ in 0..FRAMES {
        harness.run();
    }
    let csv = fs::read_to_string(&log_path)
        .unwrap_or_else(|error| panic!("perf log at {}: {error}", log_path.display()));
    let mut central_ms: Vec<f64> = csv
        .lines()
        .skip(1)
        .filter_map(|line| line.split(',').nth(4)?.parse().ok())
        .collect();
    assert!(!central_ms.is_empty(), "perf smoke recorded no frames");
    central_ms.sort_by(f64::total_cmp);
    let p95_index = ((central_ms.len() as f64 * 0.95).ceil() as usize)
        .saturating_sub(1)
        .min(central_ms.len() - 1);
    let p95 = central_ms[p95_index];
    println!(
        "Agents hooks perf smoke - {REPOS} repos x {HOOKS_PER_REPO} hooks, {FRAMES} frames: central p95 {p95:.2}ms"
    );
    assert!(
        p95 < 33.0,
        "Agents hooks central render p95 exceeded the 30fps frame budget: {p95:.2}ms"
    );
}
