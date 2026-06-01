//! The two-row top panel: title + workspace-wide controls (Refresh, Kill-all,
//! Browser picker) and a single filter input that drives the central
//! `workspace` panel below.

use crate::app::{self, HiveApp};
use crate::runtime::ViewTab;
use crate::ui::theme;
use crate::ui::workspace;
use eframe::egui;
use switchbard_core::BROWSER_APP_NAMES;

pub fn render(app: &mut HiveApp, ctx: &egui::Context) {
    egui::TopBottomPanel::top("top").show(ctx, |ui| {
        ui.horizontal(|ui| {
            ui.heading("Switchbard");
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
            ui.separator();
            render_actions(app, ui);
        });
        ui.horizontal(|ui| {
            render_view_tabs(app, ui);
            ui.separator();
            render_filter_controls(app, ui);
        });
    });
}

fn render_view_tabs(app: &mut HiveApp, ui: &mut egui::Ui) {
    ui.label("view:");
    ui.selectable_value(&mut app.view_tab, ViewTab::Servers, "Servers");
    ui.selectable_value(&mut app.view_tab, ViewTab::AgentContext, "Agent Context");
}

fn render_filter_controls(app: &mut HiveApp, ui: &mut egui::Ui) {
    ui.label("filter:");
    ui.add(egui::TextEdit::singleline(&mut app.filter).desired_width(420.0));
    ui.label(
        egui::RichText::new("matches repo, branch, service, command, port, listener cwd")
            .color(theme::MUTED_TEXT),
    );
    if app.view_tab == ViewTab::Servers {
        ui.separator();
        ui.checkbox(&mut app.show_only_managed, "only attributed listeners");
        ui.checkbox(&mut app.show_non_servers, "show non-server scripts");
    }
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

fn render_actions(app: &mut HiveApp, ui: &mut egui::Ui) {
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
        app.mark_agent_contexts_stale();
    }

    ui.separator();
    let pgids = workspace::unique_pgids_in_filter(app);
    let label = format!("Kill all in filter ({})", pgids.len());
    let enabled = !pgids.is_empty();
    if ui.add_enabled(enabled, egui::Button::new(label)).clicked() {
        app.confirm_kill_all = true;
    }

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

    ui.separator();
    render_zoom_stepper(ui);

    if let Some(msg) = app.config_status.snapshot() {
        ui.separator();
        ui.label(msg);
    }
    if let Some(msg) = app.kill_status.snapshot() {
        ui.separator();
        ui.label(msg);
    }
    if let Some(msg) = app.server_status.snapshot() {
        ui.separator();
        ui.label(msg);
    }
}

/// A compact `Zoom: A-  120%  A+` control. The percentage button resets to
/// 100%. Persistence is automatic: `HiveApp::update` reads `ctx.zoom_factor()`
/// back each frame and writes it to the config, so this stepper and the native
/// Cmd +/-/0 shortcuts share one durable source of truth.
fn render_zoom_stepper(ui: &mut egui::Ui) {
    ui.label("Zoom:");
    let zoom = ui.ctx().zoom_factor();
    if ui
        .add_enabled(zoom > app::MIN_UI_SCALE + 1e-3, egui::Button::new("A-"))
        .on_hover_text("Smaller text (Cmd -)")
        .clicked()
    {
        ui.ctx()
            .set_zoom_factor(app::clamp_ui_scale(zoom - app::UI_SCALE_STEP));
    }
    if ui
        .button(format!("{:.0}%", zoom * 100.0))
        .on_hover_text("Reset to 100% (Cmd 0)")
        .clicked()
    {
        ui.ctx().set_zoom_factor(1.0);
    }
    if ui
        .add_enabled(zoom < app::MAX_UI_SCALE - 1e-3, egui::Button::new("A+"))
        .on_hover_text("Larger text (Cmd +)")
        .clicked()
    {
        ui.ctx()
            .set_zoom_factor(app::clamp_ui_scale(zoom + app::UI_SCALE_STEP));
    }
}
