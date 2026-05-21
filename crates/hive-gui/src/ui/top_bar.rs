//! The two-row top panel: tab switcher + per-view controls (filters,
//! browser picker, Refresh button) + status badges.

use crate::app::HiveApp;
use crate::runtime::ViewMode;
use crate::ui::listeners;
use eframe::egui;
use hive_core::BROWSER_APP_NAMES;

pub fn render(app: &mut HiveApp, ctx: &egui::Context) {
    egui::TopBottomPanel::top("top").show(ctx, |ui| {
        ui.horizontal(|ui| {
            ui.heading("Hive");
            ui.separator();
            ui.selectable_value(&mut app.view, ViewMode::Listeners, "Listeners");
            ui.selectable_value(&mut app.view, ViewMode::Worktrees, "Worktrees");
            ui.selectable_value(&mut app.view, ViewMode::Servers, "Servers");
            ui.separator();
            let (last_scan, last_error, total, attributed) = scan_summary(app);
            if let Some(at) = last_scan {
                ui.label(format!("{}s since last scan", at.elapsed().as_secs()));
            } else {
                ui.label("scanning…");
            }
            if let Some(err) = &last_error {
                ui.colored_label(egui::Color32::RED, format!("error: {err}"));
            }
            ui.separator();
            ui.label(format!("{total} listeners"));
            ui.label(format!("({attributed} attributed)"));

            match app.view {
                ViewMode::Listeners => listeners_extras(app, ui),
                ViewMode::Worktrees => worktrees_extras(app, ui),
                ViewMode::Servers => servers_extras(app, ui),
            }
        });
        ui.horizontal(|ui| match app.view {
            ViewMode::Listeners => {
                ui.checkbox(
                    &mut app.show_only_managed,
                    "only attributed to a known repo",
                );
                ui.label("filter:");
                ui.text_edit_singleline(&mut app.filter);
            }
            ViewMode::Worktrees => {
                ui.label("filter:");
                ui.text_edit_singleline(&mut app.wt_filter);
                ui.label(egui::RichText::new("matches repo name, branch, or path").weak());
            }
            ViewMode::Servers => {
                ui.label("filter:");
                ui.text_edit_singleline(&mut app.server_filter);
                ui.checkbox(&mut app.show_non_servers, "show non-server scripts");
                ui.label(egui::RichText::new("matches repo, branch, service, or command").weak());
            }
        });
    });
}

fn scan_summary(app: &HiveApp) -> (Option<std::time::Instant>, Option<String>, usize, usize) {
    let s = app.state.lock().unwrap();
    let attributed = s.listeners.iter().filter(|l| l.repo_name.is_some()).count();
    (
        s.last_scan,
        s.last_error.clone(),
        s.listeners.len(),
        attributed,
    )
}

fn listeners_extras(app: &mut HiveApp, ui: &mut egui::Ui) {
    ui.separator();
    let pgids = listeners::unique_pgids_in_filter(app);
    let label = format!("Kill all in filter ({})", pgids.len());
    let enabled = !pgids.is_empty();
    if ui.add_enabled(enabled, egui::Button::new(label)).clicked() {
        app.confirm_kill_all = true;
    }
    if let Some(msg) = app.kill_status.snapshot() {
        ui.separator();
        ui.label(msg);
    }
}

fn worktrees_extras(app: &mut HiveApp, ui: &mut egui::Ui) {
    ui.separator();
    // Refresh = re-enumerate from `git worktree list` (picks up pruned/added
    // worktrees) AND re-probe their git status. The button surfaces a delta
    // string so the user knows it actually did something.
    if ui
        .button("Refresh")
        .on_hover_text("Re-enumerate worktrees from git and re-probe their status")
        .clicked()
    {
        let delta = app.refresh_worktrees_from_disk();
        app.config_status.set(delta.summary());
        app.probe_kick.notify();
        app.scanner_kick.notify();
        app.detection_kick.notify();
    }
    if let Some(msg) = app.config_status.snapshot() {
        ui.separator();
        ui.label(msg);
    }
}

fn servers_extras(app: &mut HiveApp, ui: &mut egui::Ui) {
    ui.separator();
    ui.label("Browser:");
    let current_label = match app.browser_choice {
        0 => "Default".to_string(),
        i => BROWSER_APP_NAMES
            .get(i - 1)
            .copied()
            .unwrap_or("?")
            .to_string(),
    };
    egui::ComboBox::from_id_salt("browser_choice_combo")
        .selected_text(current_label)
        .show_ui(ui, |ui| {
            ui.selectable_value(&mut app.browser_choice, 0, "Default");
            for (i, name) in BROWSER_APP_NAMES.iter().enumerate() {
                ui.selectable_value(&mut app.browser_choice, i + 1, *name);
            }
        });
    if let Some(msg) = app.server_status.snapshot() {
        ui.separator();
        ui.label(msg);
    }
}
