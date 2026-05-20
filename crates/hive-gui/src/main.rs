//! Hive — local listeners / worktree / server dashboard.
//!
//! `main` does only three things:
//! 1. Load persisted config from `~/.hive/config.toml`.
//! 2. Expand the configured repos into their live worktree list.
//! 3. Hand off to eframe to run the GUI.

use eframe::egui;
use hive_core::config;
use hive_gui::app::HiveApp;
use hive_gui::runtime::worktrees::expand_worktrees;

fn main() -> eframe::Result<()> {
    let cfg = config::load();
    let repos = cfg.repos.clone();
    let worktrees = expand_worktrees(&repos);
    eprintln!(
        "Hive: loaded {} configured repo{} ({} total worktrees) from {}",
        repos.len(),
        if repos.len() == 1 { "" } else { "s" },
        worktrees.len(),
        config::default_path()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "(no home dir)".into()),
    );

    let opts = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 760.0])
            .with_title("Hive — Local Listeners"),
        ..Default::default()
    };
    eframe::run_native(
        "Hive",
        opts,
        Box::new(|cc| Ok(Box::new(HiveApp::new(cc, cfg, repos, worktrees)))),
    )
}
