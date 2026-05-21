//! Workspace central panel — one scroll area, three collapsible sections.
//!
//! Each section keeps the column structure of the old tab it replaced:
//!
//! - **Worktrees** — per-repo tables of branch / status / drift / activity /
//!   last commit / listeners / path.
//! - **Servers**   — per-repo tables of branch / service / state / ports / actions
//!   (ResolvedService rows; entry points stacked under the service name).
//! - **Listeners** — flat-or-grouped tables of port / pid / pgid / command /
//!   repo / branch / cwd / kill button.
//!
//! Sections are `CollapsingHeader`s with `default_open(true)`; egui persists
//! the user's collapse state across frames. The top bar owns the single
//! filter input + global toggles + status messages.

use crate::app::HiveApp;
use crate::ui::{listeners, servers, worktrees};
use eframe::egui;

pub fn render(app: &mut HiveApp, ctx: &egui::Context) {
    egui::CentralPanel::default().show(ctx, |ui| {
        egui::ScrollArea::vertical()
            .id_salt("workspace_scroll")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                egui::CollapsingHeader::new(egui::RichText::new("Worktrees").heading())
                    .id_salt("workspace_section_worktrees")
                    .default_open(true)
                    .show(ui, |ui| {
                        worktrees::render(app, ui, ctx);
                    });

                ui.add_space(8.0);

                egui::CollapsingHeader::new(egui::RichText::new("Servers").heading())
                    .id_salt("workspace_section_servers")
                    .default_open(true)
                    .show(ui, |ui| {
                        servers::render(app, ui, ctx);
                    });

                ui.add_space(8.0);

                egui::CollapsingHeader::new(egui::RichText::new("Listeners").heading())
                    .id_salt("workspace_section_listeners")
                    .default_open(true)
                    .show(ui, |ui| {
                        listeners::render(app, ui, ctx);
                    });
            });
    });
}
