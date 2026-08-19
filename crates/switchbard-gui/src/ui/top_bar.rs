//! The two-row top panel: title + workspace-wide controls (Refresh, Kill-all,
//! Browser picker) and a single filter input that drives the central
//! `workspace` panel below.

use crate::app::{self, HiveApp};
use crate::runtime::ViewTab;
use crate::ui::components::action_status_label;
use crate::ui::theme;
use crate::ui::theme::ThemeChoice;
use crate::ui::workspace;
use eframe::egui;
use switchbard_core::BROWSER_APP_NAMES;

pub fn render(app: &mut HiveApp, ctx: &egui::Context) {
    egui::TopBottomPanel::top("top").show(ctx, |ui| {
        ui.horizontal(|ui| {
            ui.heading("Switchbard");
            ui.separator();
            // Owner UX pass (2026-08-05): the "Ns since last scan" label is
            // gone — staleness isn't actionable information on its own (the
            // Refresh button next to it is), and the owner found it just
            // added visual noise. If staleness ever needs to surface again,
            // do it as a subtle indicator (e.g. dimming Refresh's icon),
            // not a ticking counter competing for attention with the error
            // label right after it.
            let (last_error, total, attributed) = scan_summary(app);
            if let Some(err) = &last_error {
                ui.colored_label(theme::danger(), format!("error: {err}"));
            }
            ui.separator();
            ui.label(format!("{total} listeners"));
            ui.label(format!("({attributed} attributed)"));
            render_retired_worktrees_nudge(app, ui);
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

/// Owner UX pass (2026-08-05): the view switcher gets its own `nav_bg()`
/// band, distinct from the plain panel background the filter row sits on
/// right next to it — navigation should read as its own zone, not blend
/// into the content controls beside it.
fn render_view_tabs(app: &mut HiveApp, ui: &mut egui::Ui) {
    egui::Frame::default()
        .fill(theme::nav_bg())
        .corner_radius(4.0)
        .inner_margin(egui::Margin::symmetric(8, 3))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label("view:");
                ui.selectable_value(&mut app.view_tab, ViewTab::Servers, "Servers");
                ui.selectable_value(&mut app.view_tab, ViewTab::AgentContext, "Agent Context");
                ui.selectable_value(&mut app.view_tab, ViewTab::Backlog, "Backlog");
                // "Dispatches", not "Dispatch": the per-task flag button in the
                // Backlog detail rail is already labeled "Dispatch", and this
                // repo deliberately keeps that string unique in the
                // accessibility tree (see `detail_lists::render_dispatch`'s note
                // on why it has no section header). The plural also reads as
                // "the list of runs", which is what the view is.
                ui.selectable_value(&mut app.view_tab, ViewTab::Dispatch, "Dispatches");
            });
        });
}

fn render_filter_controls(app: &mut HiveApp, ui: &mut egui::Ui) {
    ui.label("filter:");
    ui.add(egui::TextEdit::singleline(&mut app.filter).desired_width(420.0));
    let hint = match app.view_tab {
        ViewTab::Servers => "matches repo, branch, service, command, port, listener cwd",
        ViewTab::AgentContext => "matches repo, path, title, or instruction text",
        ViewTab::Backlog => "matches task id, title, labels, assignee, or description",
        ViewTab::Dispatch => "matches task id, title, repo, or branch",
    };
    ui.label(egui::RichText::new(hint).color(theme::muted_text()));
    match app.view_tab {
        ViewTab::Servers => {
            ui.separator();
            ui.checkbox(&mut app.show_only_managed, "only attributed listeners");
            ui.checkbox(&mut app.show_non_servers, "show non-server scripts");
        }
        ViewTab::Backlog => {}
        ViewTab::AgentContext => {}
        ViewTab::Dispatch => {}
    }
}

/// TASK-41: "N retired worktrees" — nudges toward the Workspace's Merged
/// filter chip + bulk-remove sweep whenever at least one non-primary
/// worktree is clean and fully merged. Silent when the count is 0 rather
/// than showing "0 retired", matching the repo's "no ticking counters with
/// nothing to say" bias (see this module's header doc on the removed
/// last-scan label).
fn render_retired_worktrees_nudge(app: &HiveApp, ui: &mut egui::Ui) {
    let n = workspace::staleness::retired_worktree_count(app);
    if n == 0 {
        return;
    }
    ui.separator();
    ui.label(format!(
        "{n} retired worktree{}",
        if n == 1 { "" } else { "s" }
    ))
    .on_hover_text("Clean, fully-merged worktrees — see the Workspace view's Merged filter");
}

fn scan_summary(app: &HiveApp) -> (Option<String>, usize, usize) {
    let s = app.state.lock().unwrap();
    let attributed = s.listeners.iter().filter(|l| l.repo_name.is_some()).count();
    (s.last_error.clone(), s.listeners.len(), attributed)
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
        app.backlog_kick.notify();
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
    render_theme_toggle(app, ui);

    ui.separator();
    render_zoom_stepper(ui);

    ui.separator();
    // Owner UX pass (2026-08-05): repo add/remove reachable from any view,
    // now that "Tracked repos" itself only renders in the Servers view.
    if ui
        .button("⚙ Settings")
        .on_hover_text("Add or remove tracked repos")
        .clicked()
    {
        app.settings_open = true;
    }

    // TASK-28 (owner-found bug): every status message goes through the
    // same clamped label, not a bare `ui.label` — see
    // `action_status_label`'s doc for why this is defense in depth, not
    // just the fix for the one function that triggered it.
    if let Some(msg) = app.config_status.snapshot() {
        ui.separator();
        action_status_label(ui, &msg, None);
    }
    if let Some(msg) = app.kill_status.snapshot() {
        ui.separator();
        action_status_label(ui, &msg, None);
    }
    if let Some(msg) = app.server_status.snapshot() {
        ui.separator();
        action_status_label(ui, &msg, None);
    }
    if let Some(msg) = app.backlog_status.snapshot() {
        ui.separator();
        action_status_label(ui, &msg, None);
    }
}

/// Toggle between Flight Strips (light) and Operator's Console (dark),
/// task-14/task-15 AC #5. Mutates `config.ui.theme` in place; `HiveApp::
/// update` detects the change and persists it the same way it does the
/// zoom factor.
fn render_theme_toggle(app: &mut HiveApp, ui: &mut egui::Ui) {
    // Plain text, not an emoji glyph: the repo's other icons are all
    // painter-drawn (see `theme::painted_trash_button` etc.) specifically
    // because stock/embedded fonts don't reliably cover emoji code points.
    let (label, hover) = match app.config.ui.theme {
        ThemeChoice::Light => ("Dark theme", "Switch to Operator's Console (dark)"),
        ThemeChoice::Dark => ("Light theme", "Switch to Flight Strips (light)"),
    };
    if ui.button(label).on_hover_text(hover).clicked() {
        app.config.ui.theme = app.config.ui.theme.toggled();
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
