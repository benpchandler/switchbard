//! Shared filter controls used across Switchbard's top-level views.
//!
//! This module owns the presentation and interaction conventions for filters:
//! labels, search fields, facet selectors, active counts, and reset actions.
//! Views still own their domain-specific state and matching predicates.

use crate::ui::theme;
use eframe::egui;

/// Render a consistent, wrapping filter surface.
pub fn bar<R>(
    ui: &mut egui::Ui,
    active_count: usize,
    add_contents: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::InnerResponse<R> {
    egui::Frame::NONE
        .fill(theme::nav_bg())
        .stroke(theme::surface_stroke())
        .corner_radius(6.0)
        .inner_margin(egui::Margin::symmetric(9, 6))
        .show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(egui::RichText::new("Filters").strong());
                if active_count > 0 {
                    ui.label(
                        egui::RichText::new(format!("{active_count} active"))
                            .small()
                            .color(theme::lavender()),
                    );
                }
                ui.separator();
                add_contents(ui)
            })
            .inner
        })
}

/// Render a consistently sized search field with a stable accessibility hint.
pub fn search(
    ui: &mut egui::Ui,
    id_salt: &'static str,
    value: &mut String,
    hint: &str,
) -> egui::Response {
    ui.label(egui::RichText::new("Search").color(theme::muted_text()));
    let width = search_width(ui.available_width());
    ui.add(
        egui::TextEdit::singleline(value)
            .id_salt(id_salt)
            .hint_text(hint)
            .desired_width(width),
    )
}

fn search_width(available_width: f32) -> f32 {
    available_width.clamp(180.0, 360.0)
}

/// Render the shared facet label treatment.
pub fn facet_label(ui: &mut egui::Ui, label: &str) {
    ui.label(egui::RichText::new(label).color(theme::muted_text()));
}

/// Render the shared reset action. Returns whether it was clicked.
pub fn clear(ui: &mut egui::Ui, enabled: bool) -> bool {
    ui.add_enabled(enabled, egui::Button::new("Clear filters"))
        .on_hover_text("Restore every filter on this page to its default")
        .clicked()
}

#[cfg(test)]
mod tests {
    use super::search_width;

    #[test]
    fn search_width_is_bounded_for_narrow_and_wide_rows() {
        assert_eq!(search_width(80.0), 180.0);
        assert_eq!(search_width(900.0), 360.0);
    }
}
