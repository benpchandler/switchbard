//! Switchbard — local listeners / worktree / server dashboard.
//!
//! `main` does only three things:
//! 1. Load persisted config from `~/.switchbard/config.toml`.
//! 2. Expand the configured repos into their live worktree list.
//! 3. Hand off to eframe to run the GUI.

use std::sync::Arc;

use eframe::egui;
use switchbard_core::config;
use switchbard_gui::app::HiveApp;
use switchbard_gui::runtime::worktrees::expand_worktrees;

/// 1024×1024 source PNG baked into the binary. The same asset is also packaged
/// as `Contents/Resources/icon.icns` when we ship a `.app` bundle (see
/// `scripts/bundle-mac.sh`). Both paths exist on purpose — the embedded PNG
/// drives the runtime window + Dock icon under `cargo run`; the `.icns`
/// drives Finder/Launchpad/Dock for the installed bundle.
const APP_ICON_PNG: &[u8] = include_bytes!("../assets/icon.png");

fn main() -> eframe::Result<()> {
    let cfg = config::load();
    let repos = cfg.repos.clone();
    let worktrees = expand_worktrees(&repos);
    eprintln!(
        "Switchbard: loaded {} configured repo{} ({} total worktrees) from {}",
        repos.len(),
        if repos.len() == 1 { "" } else { "s" },
        worktrees.len(),
        config::default_path()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "(no home dir)".into()),
    );

    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size([1280.0, 760.0])
        .with_title("Switchbard — Local Listeners");
    match eframe::icon_data::from_png_bytes(APP_ICON_PNG) {
        Ok(icon) => viewport = viewport.with_icon(Arc::new(icon)),
        Err(e) => eprintln!("Switchbard: failed to load app icon: {e}"),
    }

    let opts = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };
    eframe::run_native(
        "Switchbard",
        opts,
        Box::new(|cc| Ok(Box::new(HiveApp::new(cc, cfg, repos, worktrees)))),
    )
}
