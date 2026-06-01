//! Small badges and missing-data placeholders.
//!
//! These appeared inline 15+ times across the view modules as
//! `RichText::new("—")` colored `theme::WEAK_TEXT`. Centralized
//! so a future restyle of "what does missing data look like" is one diff.

use eframe::egui;

use crate::ui::theme;

/// "—" — the value is known to be absent or in-sync. Always subdued.
pub fn weak_dash(ui: &mut egui::Ui) {
    ui.label(egui::RichText::new("—").color(theme::WEAK_TEXT));
}

/// "…" — the value is being computed / probe in flight. Always subdued.
pub fn weak_dots(ui: &mut egui::Ui) {
    ui.label(egui::RichText::new("…").color(theme::WEAK_TEXT));
}

/// Count badge for table cells. Renders the number in `color` when > 0,
/// otherwise renders a weak dash. The "0 listeners" case is information-
/// poor; a dash reads as "nothing to see here" without flagging it.
pub fn count_badge(ui: &mut egui::Ui, n: usize, color: egui::Color32) {
    if n > 0 {
        ui.colored_label(color, n.to_string());
    } else {
        weak_dash(ui);
    }
}
