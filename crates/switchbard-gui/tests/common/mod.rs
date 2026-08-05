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
    // Mark onboarding dismissed: otherwise `should_show` (no config repos +
    // not dismissed) fires the first-launch modal, which spawns a *real* scan
    // of `~/` for git repos — non-hermetic and non-deterministic. Suppressing
    // it keeps the harness driving only the seeded view.
    let mut cfg = Config::default();
    cfg.ui.onboarding_dismissed = true;
    let mut app = HiveApp::new_headless(cfg, repos, worktrees);
    // MUST be set on every test-constructed HiveApp: `save_config` targets
    // the real `~/.switchbard/config.toml` by default (correct for
    // production), and any test that clicks a Save/Delete-style button
    // (e.g. saved_views' "Save current as…") reaches it. Forgetting this is
    // exactly how TASK-22 happened — every `cargo test` run was silently
    // overwriting the developer's real tracked-repo list. `isolated_config_
    // save_path` gives each call a fresh, unique path under `$TMPDIR` so
    // parallel test threads can't collide either.
    app.config_save_path = Some(isolated_config_save_path());
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

/// A fresh, unique path under `$TMPDIR` for a test's `HiveApp::config_save_
/// path` — never the real `~/.switchbard/config.toml`. Unique per call (not
/// just per process) because `cargo test` runs test functions in parallel
/// threads within one process, so a shared path would let two tests race on
/// the same file.
pub fn isolated_config_save_path() -> PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "switchbard-test-config-{}-{n}.toml",
        std::process::id()
    ))
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
                // `render_ui` itself calls `theme::apply(ctx, self.config.ui.theme)`
                // every frame now (needed in production for the live toggle), so
                // the headless path — which skips `HiveApp::new`'s one-time font
                // install — gets correct visuals for free; embedded fonts aren't
                // needed for these tests (legibility_audit measures requested
                // point size and color, not glyph shapes).
                app.render_ui(ctx);
            },
            app,
        )
}
