//! Shared fixtures for the egui_kittest UI tests.
//!
//! These build a fully-seeded [`HiveApp`] with **no** worker threads (via
//! `HiveApp::new_headless`) so a `Harness` can drive the real Switchbard views
//! against deterministic in-memory state. Seed the agent-context map yourself
//! and let `render_ui` paint it — no filesystem, no `lsof`, no git.
//!
//! `#![allow(dead_code)]` because each test binary that pulls this module in
//! uses only the helpers it needs, and unused-helper warnings would otherwise
//! fail the build under `-D warnings`.
#![allow(dead_code)]

use std::path::PathBuf;
use std::time::SystemTime;

use eframe::egui;
use egui_kittest::Harness;
use switchbard_core::config::Config;
use switchbard_core::{
    AgentContextItem, AgentContextMap, AgentKind, ContextKind, ContextScope, Repo, WorktreeRef,
};
use switchbard_gui::app::HiveApp;

pub const REPO_NAME: &str = "demo";
pub const REPO_PATH: &str = "/tmp/switchbard-ui-test/demo";

/// Build a single agent-context item with sensible defaults; override the
/// fields that matter to the test via the arguments.
pub fn item(
    id: &str,
    agent: AgentKind,
    scope: ContextScope,
    kind: ContextKind,
    title: &str,
) -> AgentContextItem {
    AgentContextItem {
        id: id.to_string(),
        agent,
        scope,
        kind,
        path: PathBuf::from(format!("{REPO_PATH}/{title}")),
        applies_to: None,
        title: title.to_string(),
        size_bytes: 1024,
        modified_at: Some(SystemTime::UNIX_EPOCH),
        warning: None,
    }
}

/// A `HiveApp` seeded with one repo + one worktree whose agent-context map
/// holds `items`. No workers are spawned.
pub fn app_with_items(items: Vec<AgentContextItem>) -> HiveApp {
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
    let app = HiveApp::new_headless(Config::default(), repos, worktrees);
    app.agent_contexts.lock().unwrap().insert(
        PathBuf::from(REPO_PATH),
        AgentContextMap {
            worktree: PathBuf::from(REPO_PATH),
            items,
            scanned_at: Some(SystemTime::UNIX_EPOCH),
        },
    );
    app
}

/// A representative fixture: one repo with a `CLAUDE.md` instruction and a
/// skill, both local-scope and attributed to Claude so they're visible under
/// the explorer's default (Local / Claude) selection.
pub fn seeded_app() -> HiveApp {
    app_with_items(vec![
        item(
            "claude-md",
            AgentKind::Claude,
            ContextScope::Local,
            ContextKind::Instruction,
            "CLAUDE.md",
        ),
        item(
            "review-skill",
            AgentKind::Claude,
            ContextScope::Local,
            ContextKind::Skill,
            "review",
        ),
    ])
}

/// Mount the full Switchbard window (every panel) for `app` in a kittest
/// `Harness`. Query the result with `kittest::Queryable`, drive it with
/// `.click()` / `.run()`, and read back state via `harness.state()`.
pub fn harness(app: HiveApp) -> Harness<'static, HiveApp> {
    Harness::builder()
        .with_size(egui::vec2(1280.0, 860.0))
        .build_state(
            |ctx, app| {
                // Match the real window: `HiveApp::new` applies the theme via
                // the CreationContext, which the headless path skips. Cheap and
                // idempotent (sets visuals only), so re-applying each frame is
                // fine and keeps snapshots faithful to production.
                switchbard_gui::ui::theme::apply(ctx);
                app.render_ui(ctx);
            },
            app,
        )
}
