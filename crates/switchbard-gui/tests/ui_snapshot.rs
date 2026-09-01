//! Opt-in pixel snapshots of the Agents view, for visual-regression work.
//!
//! This is `#[ignore]`d on purpose. It renders through `wgpu` (a real GPU
//! adapter) and compares against a committed PNG baseline — which is sensitive
//! to GPU/driver/font differences across machines, so it is **not** part of the
//! CI gate. CI still *compiles* it (catching API breakage); it just doesn't run
//! it. The accesskit interaction tests in `ui_views.rs` are the durable,
//! deterministic layer.
//!
//! Workflow when you want visual regression locally — create/refresh the
//! baseline, then validate against it:
//!
//! ```sh
//! UPDATE_SNAPSHOTS=1 cargo test -p switchbard-gui --test ui_snapshot -- --ignored
//! cargo test -p switchbard-gui --test ui_snapshot -- --ignored
//! ```
//!
//! Baselines land in `tests/snapshots/<name>.png`; the `.new.png` / `.diff.png`
//! outputs are gitignored.

mod common;

use std::path::PathBuf;

use common::{harness, seeded_app};
use switchbard_core::{AgentHook, AgentKind, ContextScope};
use switchbard_gui::runtime::{AgentsSection, Place};

#[test]
#[ignore = "wgpu image snapshot: machine-specific, run explicitly with `-- --ignored` (see module docs)"]
fn agents_context_view_snapshot() {
    let mut app = seeded_app();
    app.place = Place::Command;
    let mut harness = harness(app);
    harness.run();
    harness.snapshot("agent_context_view");
}

#[test]
#[ignore = "wgpu image snapshot: machine-specific, run explicitly with `-- --ignored` (see module docs)"]
fn agents_hooks_view_snapshot() {
    let mut app = seeded_app();
    app.place = Place::Command;
    app.agent_context_view.section = AgentsSection::Hooks;
    let hooks = vec![
        AgentHook {
            id: "global-pre-tool".to_string(),
            agent: AgentKind::Claude,
            scope: ContextScope::Global,
            source_path: PathBuf::from("/Users/demo/.claude/settings.json"),
            event: "PreToolUse".to_string(),
            matcher: Some("Write|Edit".to_string()),
            hook_type: "command".to_string(),
            action: "python3".to_string(),
            arguments: vec!["/Users/demo/.claude/hooks/check-write.py".to_string()],
            condition: Some("Edit(*.rs)".to_string()),
            asynchronous: false,
            timeout_seconds: Some(30),
        },
        AgentHook {
            id: "repo-stop".to_string(),
            agent: AgentKind::Claude,
            scope: ContextScope::Local,
            source_path: PathBuf::from("/tmp/switchbard-ui-test/demo/.claude/settings.local.json"),
            event: "Stop".to_string(),
            matcher: None,
            hook_type: "command".to_string(),
            action: "./scripts/rebuild-and-reload.sh".to_string(),
            arguments: Vec::new(),
            condition: None,
            asynchronous: true,
            timeout_seconds: None,
        },
    ];
    app.agent_contexts
        .lock()
        .expect("invariant: seeded agent context cache")
        .values_mut()
        .next()
        .expect("invariant: seeded worktree context")
        .hooks = hooks;
    let mut harness = harness(app);
    harness.run();
    harness.snapshot("agents_hooks_view");
}

#[test]
#[ignore = "wgpu image snapshot: machine-specific, run explicitly with `-- --ignored` (see module docs)"]
fn agents_hooks_active_filters_narrow_snapshot() {
    let mut app = seeded_app();
    app.place = Place::Command;
    app.agent_context_view.section = AgentsSection::Hooks;
    app.agent_context_view.hook_scope = Some(ContextScope::Local);
    app.agent_context_view.hook_event = Some("PostToolUse".to_string());
    *app.filter_mut() = "after-edit".to_string();
    app.agent_contexts
        .lock()
        .expect("invariant: seeded agent context cache")
        .values_mut()
        .next()
        .expect("invariant: seeded worktree context")
        .hooks = vec![AgentHook {
        id: "repo-post-tool".to_string(),
        agent: AgentKind::Claude,
        scope: ContextScope::Local,
        source_path: PathBuf::from("/tmp/switchbard-ui-test/demo/.claude/settings.local.json"),
        event: "PostToolUse".to_string(),
        matcher: Some("Write|Edit".to_string()),
        hook_type: "command".to_string(),
        action: "./scripts/after-edit-with-a-long-unbroken-action-name.sh".to_string(),
        arguments: Vec::new(),
        condition: None,
        asynchronous: false,
        timeout_seconds: Some(30),
    }];
    let mut harness = harness(app);
    harness.set_size(eframe::egui::vec2(720.0, 620.0));
    harness.run();
    harness.snapshot("agents_hooks_active_filters_narrow");
}
