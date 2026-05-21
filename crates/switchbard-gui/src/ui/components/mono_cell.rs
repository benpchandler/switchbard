//! Monospace text cells. Used for things you want the eye to scan column-wise:
//! short SHAs, drift counts, port numbers, command strings.

use eframe::egui;

/// First 8 chars of a SHA, monospace. The "8" choice matches what most git
/// UIs render (`git log --oneline`).
pub fn short_sha(ui: &mut egui::Ui, sha: &str) -> egui::Response {
    let short = if sha.len() >= 8 { &sha[..8] } else { sha };
    ui.label(egui::RichText::new(short).monospace())
}

/// Generic monospace cell with an optional color tint. Use this for values
/// where the eye should track digits / fixed-width tokens.
pub fn mono_label(ui: &mut egui::Ui, text: &str, color: Option<egui::Color32>) -> egui::Response {
    let mut rich = egui::RichText::new(text).monospace();
    if let Some(c) = color {
        rich = rich.color(c);
    }
    ui.label(rich)
}
