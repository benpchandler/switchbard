//! All semantic colors and a handful of glyph constants used across the GUI.
//!
//! Centralizing these means a future palette change is a one-file diff, and
//! it stops "what does Color32::from_rgb(120, 230, 140) mean?" from being a
//! recurring "grep the call sites" exercise.

use eframe::egui::Color32;

// Green ≈ healthy / running / has-listeners.
pub const GREEN: Color32 = Color32::from_rgb(120, 230, 140);
// Amber ≈ dirty / ambiguous classifier verdict ("Maybe").
pub const AMBER: Color32 = Color32::from_rgb(230, 180, 100);
// Soft amber used for the Servers classifier "Maybe" dot — slightly hotter
// to read as a question, not a warning.
pub const AMBER_QUESTION: Color32 = Color32::from_rgb(230, 200, 100);
// Lavender ≈ ahead/behind drifted from origin.
pub const LAVENDER: Color32 = Color32::from_rgb(180, 180, 240);
// Sky blue ≈ external-live: bound but not by us.
pub const SKY: Color32 = Color32::from_rgb(120, 200, 240);
// Orange ≈ blocked / port-conflict warning.
pub const WARN_ORANGE: Color32 = Color32::from_rgb(240, 130, 90);
// Red used for the destructive "Kill all" / "Stop" / "Confirm" buttons.
pub const DANGER: Color32 = Color32::from_rgb(180, 60, 60);

// Glyphs (single source of truth — used in headings, dots, badges).
pub const DOT_FILLED: &str = "●";
pub const DOT_HOLLOW: &str = "○";
pub const DOT_SMALL: &str = "•";
