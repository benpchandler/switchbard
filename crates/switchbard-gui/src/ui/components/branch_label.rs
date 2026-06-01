//! Branch name with consistent "(detached)" handling across views.

use eframe::egui;

use crate::ui::theme;

/// Render a branch name. `None` means detached HEAD — rendered italic+weak so
/// it's clearly distinct from a named branch. The label truncates with an
/// ellipsis when the column is narrower than the branch — full name is shown
/// in the hover tooltip so no information is lost.
pub fn branch_label(ui: &mut egui::Ui, branch: Option<&str>) -> egui::Response {
    match branch {
        Some(name) => ui
            .add(egui::Label::new(name).truncate())
            .on_hover_text(name),
        None => ui.add(
            egui::Label::new(
                egui::RichText::new("(detached)")
                    .italics()
                    .color(theme::MUTED_TEXT),
            )
            .truncate(),
        ),
    }
}
