//! Per-repo section header and inter-section separator.
//!
//! Worktrees and Servers both render a stack of per-repo tables. Each gets
//! the same chrome: a heading with "(N wt)" subtitle, optional colored
//! chips, an optional subtitle line. Between sections, a hairline + breathing
//! room. Centralizing both gives every stacked-section view the same rhythm.

use eframe::egui;

/// A colored badge in the section header, like "2 listening" or "1 dirty".
pub struct Chip<'a> {
    pub color: egui::Color32,
    pub text: &'a str,
}

/// Render a `<heading>  (N subtitle)  · chip · chip · chip` row + a weak
/// subtitle line beneath it. The chips are filtered for non-empty content
/// by the caller; pass an empty slice to skip the chip strip entirely.
pub fn repo_section_header(
    ui: &mut egui::Ui,
    heading: &str,
    subtitle: &str,
    chips: &[Chip<'_>],
    second_line: Option<&str>,
) {
    ui.horizontal(|ui| {
        ui.heading(heading);
        ui.label(egui::RichText::new(subtitle).weak());
        if !chips.is_empty() {
            ui.separator();
        }
        for c in chips {
            ui.colored_label(c.color, c.text);
        }
    });
    if let Some(line) = second_line {
        ui.label(egui::RichText::new(line).weak());
    }
    ui.add_space(6.0);
}

/// Hairline + breathing room rendered between repo sections. The first
/// visible section in a tab passes `first = true` and gets no separator
/// (the tab chrome above already separates it). After this call the caller
/// should set `first = false`.
///
/// Returns the new value of `first` so the caller can simply do:
/// ```ignore
/// first = repo_section_separator(ui, first);
/// ```
pub fn repo_section_separator(ui: &mut egui::Ui, first: bool) -> bool {
    if !first {
        ui.add_space(16.0);
        ui.separator();
        ui.add_space(12.0);
    }
    false
}
