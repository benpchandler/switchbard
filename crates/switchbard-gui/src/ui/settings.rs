//! A small "Settings" window (owner UX pass, 2026-08-05) reachable from any
//! view via the top bar's "⚙ Settings" button — repo add/remove no longer
//! require switching to the Servers view now that "Tracked repos" moved
//! there as a left-side, Servers-local panel (`ui::sidebar`). Calls the
//! exact same `HiveApp` methods that panel's own add/remove buttons do
//! (`open_repo_picker`, `remove_repo` via `confirm_remove_repo`) — a
//! different surface for the same actions, not a parallel implementation.
//! Worktree/listener detail stays Servers-only; this is deliberately just
//! the repo list.

use crate::app::HiveApp;
use crate::runtime::PickerState;
use crate::ui::theme;
use eframe::egui;

pub fn render_settings_window(app: &mut HiveApp, ui: &mut egui::Ui) {
    let ctx = &ui.ctx().clone();
    if !app.settings_open {
        return;
    }
    let repos = app.repos_snapshot();
    let picker_busy = matches!(*app.picker.lock().unwrap(), PickerState::InFlight);
    let mut want_pick = false;
    let mut open = true;

    egui::Window::new("Settings")
        .id(egui::Id::new("settings_window"))
        .open(&mut open)
        .collapsible(false)
        .resizable(true)
        .default_width(420.0)
        .show(ctx, |ui| {
            ui.heading("Tracked repos");
            ui.label(
                egui::RichText::new(
                    "Add or remove repos from Switchbard — this doesn't touch anything on disk.",
                )
                .small()
                .color(theme::muted_text()),
            );
            ui.add_space(8.0);
            let label = if picker_busy {
                "Picking…"
            } else {
                "➕ Add repo"
            };
            if ui
                .add_enabled(!picker_busy, egui::Button::new(label))
                .on_hover_text("Choose a folder containing a git repository")
                .clicked()
            {
                want_pick = true;
            }
            ui.add_space(6.0);

            if repos.is_empty() {
                ui.label(egui::RichText::new("No repos configured yet").color(theme::muted_text()));
            } else {
                let last = repos.len() - 1;
                for (i, repo) in repos.iter().enumerate() {
                    ui.horizontal(|ui| {
                        ui.label(&repo.name);
                        // Settings is the sanctioned home for repo CRUD
                        // (module doc); reorder is CRUD's missing verb —
                        // `HiveApp::move_repo` has existed since before the
                        // old sidebar panel's up/down triangles were retired
                        // with it, but nothing in this window ever called it
                        // (TASK-100 medic pass). Reuses the same
                        // `theme::triangle_button` the Backlog project list's
                        // `rank_arrows` already established as this repo's
                        // one reorder affordance, rather than inventing a
                        // second one.
                        render_reorder_controls(ui, app, i, last);
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui
                                .small_button("Remove")
                                .on_hover_text(format!(
                                    "Remove '{}' from Switchbard (confirms before removing; \
                                     does not delete the repo on disk)",
                                    repo.name
                                ))
                                .clicked()
                            {
                                app.confirm_remove_repo =
                                    Some((repo.path.clone(), repo.name.clone()));
                            }
                            ui.label(
                                egui::RichText::new(repo.path.display().to_string())
                                    .small()
                                    .color(theme::muted_text()),
                            );
                        });
                    });
                }
            }
        });

    if !open {
        app.settings_open = false;
    }
    if want_pick {
        app.open_repo_picker(ctx);
    }
}

/// The ▲▼ pair for one repo row, wired straight to `HiveApp::move_repo`.
/// `last` is `repos.len() - 1` (the caller's snapshot, not re-read here) so
/// "already at the bottom" disables the down arrow without this function
/// needing its own copy of the list. Each button carries its own AccessKit
/// label (`triangle_button` alone doesn't — callers that need one, like this
/// test-covered reorder control, attach it themselves) rather than leaving
/// it to be found by screen position, the way the Ops table's un-labeled
/// icons still have to be.
fn render_reorder_controls(ui: &mut egui::Ui, app: &mut HiveApp, i: usize, last: usize) {
    let can_up = i > 0;
    let up = theme::triangle_button(ui, true, can_up);
    up.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, can_up, "Move repo up"));
    if can_up {
        if up
            .on_hover_text("Move this repo up the tracked list")
            .clicked()
        {
            app.move_repo(i, -1);
        }
    } else {
        up.on_hover_text("Already at the top of the tracked list");
    }

    let can_down = i < last;
    let down = theme::triangle_button(ui, false, can_down);
    down.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, can_down, "Move repo down")
    });
    if can_down {
        if down
            .on_hover_text("Move this repo down the tracked list")
            .clicked()
        {
            app.move_repo(i, 1);
        }
    } else {
        down.on_hover_text("Already at the bottom of the tracked list");
    }
}
