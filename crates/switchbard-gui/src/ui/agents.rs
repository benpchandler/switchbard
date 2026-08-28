//! Top-level Agents view and its Context / Hooks information architecture.

use crate::app::HiveApp;
use crate::runtime::AgentsSection;
use crate::ui::{agent_context, agent_hooks, theme};
use eframe::egui;

pub fn render(app: &mut HiveApp, ui: &mut egui::Ui) {
    egui::CentralPanel::default().show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.heading("Agents");
            ui.label(
                egui::RichText::new("what agents read and run in each repo")
                    .color(theme::muted_text()),
            );
        });
        ui.add_space(4.0);
        render_section_switcher(app, ui);
        ui.add_space(8.0);
        match app.agent_context_view.section {
            AgentsSection::Context => agent_context::render(app, ui),
            AgentsSection::Hooks => agent_hooks::render(app, ui),
        }
    });
}

fn render_section_switcher(app: &mut HiveApp, ui: &mut egui::Ui) {
    egui::Frame::NONE
        .fill(theme::card_bg())
        .stroke(theme::surface_stroke())
        .corner_radius(6.0)
        .inner_margin(egui::Margin::symmetric(6, 4))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.selectable_value(
                    &mut app.agent_context_view.section,
                    AgentsSection::Context,
                    "Context",
                );
                ui.selectable_value(
                    &mut app.agent_context_view.section,
                    AgentsSection::Hooks,
                    "Hooks",
                );
            });
        });
}
