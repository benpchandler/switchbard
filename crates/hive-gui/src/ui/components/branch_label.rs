//! Branch name with consistent "(detached)" handling across views.

use eframe::egui;

/// Render a branch name. `None` means detached HEAD — rendered italic+weak so
/// it's clearly distinct from a named branch.
pub fn branch_label(ui: &mut egui::Ui, branch: Option<&str>) -> egui::Response {
    match branch {
        Some(name) => ui.label(name),
        None => ui.label(egui::RichText::new("(detached)").italics().weak()),
    }
}
