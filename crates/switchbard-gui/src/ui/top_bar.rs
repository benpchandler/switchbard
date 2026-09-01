//! The two-row top panel: title + workspace-wide controls (Refresh, Kill-all,
//! Browser picker) and a single filter input that drives the central
//! `workspace` panel below.

use crate::app::{self, HiveApp};
use crate::runtime::{Place, TasksView};
use crate::ui::components::action_status_label;
use crate::ui::dispatch::DispatchSummary;
use crate::ui::filter_bar;
use crate::ui::theme;
use crate::ui::theme::ThemeChoice;
use crate::ui::workspace;
use eframe::egui;
use switchbard_core::BROWSER_APP_NAMES;

/// `dispatch_summary` is computed exactly once per frame by
/// `HiveApp::render_ui` and passed to every reader (this chip, the nav
/// badge, the nav footer lamp) — see that call site's comment and
/// `ui::dispatch::summarize_dispatch`'s own doc for why it must not be
/// recomputed here.
pub(crate) fn render(app: &mut HiveApp, ui: &mut egui::Ui, dispatch_summary: DispatchSummary) {
    let ctx = &ui.ctx().clone();
    let frame = egui::Frame::side_top_panel(&ctx.style_of(ctx.theme()))
        .fill(theme::nav_bg())
        .inner_margin(egui::Margin::symmetric(10, 7))
        .stroke(theme::surface_stroke());
    egui::Panel::top("top").frame(frame).show(ui, |ui| {
        ui.horizontal_wrapped(|ui| {
            ui.label(egui::RichText::new("Switchbard").heading().strong());
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
            render_dispatch_chip(app, ui, dispatch_summary);
            ui.separator();
            render_actions(app, ui);
        });
        // IA V2 (TASK-96): the "view:" tab row is gone — the places sidebar
        // (`ui::nav`) is the sole navigation surface now. The per-place
        // filter controls row survives, keyed off `app.place` (and, under
        // Tasks, `app.tasks_view`) instead of the old `ViewTab`, and only
        // renders for the places that actually have a top-bar-level filter:
        // Ops and Tasks. Command keeps its own richer facet bar
        // (`ui::agents::render_filters`); Digest and Goals are read-only
        // summary bodies with nothing here to filter.
        if matches!(app.place, Place::Ops | Place::Tasks) {
            ui.add_space(3.0);
            ui.horizontal_wrapped(|ui| {
                render_filter_controls(app, ui);
            });
        }
    });
}

fn render_filter_controls(app: &mut HiveApp, ui: &mut egui::Ui) {
    let hint = match app.place {
        Place::Ops => "matches repo, branch, service, command, port, listener cwd",
        Place::Tasks => match app.tasks_view {
            TasksView::All => "matches task id, title, labels, assignee, or description",
            TasksView::Dispatches => "matches task id, title, repo, or branch",
        },
        Place::Digest | Place::Command | Place::Goals => "",
    };
    filter_bar::search(ui, "top_bar_filter", app.filter_mut(), hint);
    match app.place {
        Place::Ops => {
            ui.separator();
            ui.checkbox(&mut app.show_only_managed, "only attributed listeners");
            ui.checkbox(&mut app.show_non_servers, "show non-server scripts");
            // "Default" means the shipped `UiConfig` defaults: non-server
            // scripts hidden, no staleness narrowing. The staleness chips
            // render in the workspace body below, but they are still this
            // page's filter state, so the page-wide reset covers them too.
            let active = !app.filter().is_empty()
                || app.show_only_managed
                || app.show_non_servers
                || app.staleness_filter != workspace::staleness::StalenessFilter::All;
            if filter_bar::clear(ui, active) {
                app.filter_mut().clear();
                app.show_only_managed = false;
                app.show_non_servers = false;
                app.staleness_filter = workspace::staleness::StalenessFilter::All;
            }
        }
        Place::Tasks => {
            if filter_bar::clear(ui, !app.filter().is_empty()) {
                app.filter_mut().clear();
            }
        }
        Place::Digest | Place::Command | Place::Goals => {}
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

/// TASK-43: the ambient dispatch chip — the one place in the always-visible
/// chrome that says a headless agent is running, from whichever tab the user
/// is actually on.
///
/// Follows `render_retired_worktrees_nudge` exactly: silent when there is
/// nothing to say (no "0 running"), one compact label, one hover explaining
/// it. It differs in being *clickable*, because unlike a retired worktree an
/// in-flight run has a destination — the Dispatches tab, which is where every
/// control over it lives.
///
/// Two visual registers, not one with a variable color: accent for "this is
/// working", danger for "this needs you". A run that failed, was orphaned, or
/// blew past its advisory staleness threshold is not a louder version of a
/// healthy run; it is a different message, so it gets different words as well
/// as a different color (see `DispatchSummary::chip_text`). TASK-46: the
/// staleness case is still a *running* run, never one about to be killed —
/// nothing here (or anywhere) kills a run for wall-clock time.
fn render_dispatch_chip(app: &mut HiveApp, ui: &mut egui::Ui, summary: DispatchSummary) {
    if summary.is_idle() {
        return;
    }
    ui.separator();
    let color = if summary.needs_attention() {
        theme::danger()
    } else {
        theme::dispatch_accent()
    };
    let hover = if summary.needs_attention() {
        "Dispatch runs that failed, were never released, or are past their advisory staleness threshold — open the Dispatches view"
    } else {
        "Headless agent runs in flight — open the Dispatches view"
    };
    // `.frame(false)`: a *frameless* button so the text composites against the
    // panel, not against egui's button fill. `theme::danger()` and
    // `theme::dispatch_accent()` are both tuned for AA against `panel_fill`
    // (see `theme::Palette`'s doc on why the danger *text* role and the danger
    // *button fill* are deliberately different colors) — inside a filled
    // button, danger-on-button-fill measures 3.9:1 in Operator's Console and
    // `legibility_audit` fails the build. Frameless keeps this a clickable
    // label, which is also what it reads as next to the retired-worktrees
    // nudge it sits beside.
    if ui
        .add(
            egui::Button::new(
                egui::RichText::new(summary.chip_text())
                    .strong()
                    .color(color),
            )
            .frame(false),
        )
        .on_hover_text(hover)
        .clicked()
    {
        app.place = Place::Tasks;
        app.tasks_view = TasksView::Dispatches;
    }
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
