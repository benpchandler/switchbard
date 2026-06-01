//! Path cell — elided single-line display with the full path in a hover.
//!
//! Long worktree paths blow out columns; wrapping them onto multiple lines
//! makes row heights inconsistent and adds visual noise. This component is
//! the canonical answer: render a tight elided form (last two components
//! prefixed with "…/"), and put the absolute path in the tooltip so no info
//! is lost.

use crate::ui::path_display;
use crate::ui::theme;
use eframe::egui;
use std::path::Path;

/// Render a path as a weak, single-line, elided label. Hovering shows the
/// full absolute path.
pub fn path_cell(ui: &mut egui::Ui, path: &Path) -> egui::Response {
    let shown = path_display::shorten(path);
    let full = path.display().to_string();
    let resp = ui.label(egui::RichText::new(shown).color(theme::MUTED_TEXT));
    // Only attach a tooltip when we actually elided. If the shown text IS
    // the full path, the tooltip would duplicate what the user is reading.
    if path_display::shorten(path) != full {
        resp.on_hover_text(full)
    } else {
        resp
    }
}
