//! All semantic colors and a handful of glyph constants used across the GUI.
//!
//! Centralizing these means a future palette change is a one-file diff, and
//! it stops "what does Color32::from_rgb(120, 230, 140) mean?" from being a
//! recurring "grep the call sites" exercise.
//!
//! Contrast targets — every chromatic constant below hits **WCAG AA (≥4.5:1)**
//! against egui's default light panel background (≈ #F8F8F8, L ≈ 0.91). The
//! previous palette was tuned for a dark theme and washed out on light: GREEN
//! 120,230,140 measured 1.43:1, LAVENDER 180,180,240 measured 1.80:1.
//!
//! `apply(ctx)` also installs a tuned `Visuals` that darkens egui's built-in
//! `weak_text_color` so `.weak()` labels (paths, hints) reach AA too.

use eframe::egui::{self, Color32};

// Green ≈ healthy / running / has-listeners. (#117A33, 5.0:1)
pub const GREEN: Color32 = Color32::from_rgb(0x11, 0x7A, 0x33);
// Amber ≈ dirty / ambiguous classifier verdict. (#946000, 4.9:1)
pub const AMBER: Color32 = Color32::from_rgb(0x94, 0x60, 0x00);
// Soft amber used for the Servers classifier "Maybe" dot. Slightly warmer hue
// than AMBER but same luminance band so it still reads as a question, not a
// warning. (#A65A00, 4.5:1)
pub const AMBER_QUESTION: Color32 = Color32::from_rgb(0xA6, 0x5A, 0x00);
// Indigo ≈ ahead/behind drifted from origin. Replaces the unreadable pastel
// lavender. (#3F3FB0, 5.6:1)
pub const LAVENDER: Color32 = Color32::from_rgb(0x3F, 0x3F, 0xB0);
// Blue ≈ external-live: bound but not by us. (#1A6BB3, 5.0:1)
pub const SKY: Color32 = Color32::from_rgb(0x1A, 0x6B, 0xB3);
// Orange-red ≈ blocked / port-conflict warning. (#B83A0A, 5.3:1)
pub const WARN_ORANGE: Color32 = Color32::from_rgb(0xB8, 0x3A, 0x0A);
// Red used for the destructive "Kill all" / "Stop" / "Confirm" buttons.
// (#B43C3C, 5.3:1 against white text on the button)
pub const DANGER: Color32 = Color32::from_rgb(0xB4, 0x3C, 0x3C);

// Subdued text used by `.weak()` labels. egui's default is ~gray(128) which
// only hits 3.0:1 against the panel bg — fine for "decorative" gray text in
// other apps, too light here for paths and hint text. (#4A4A4A, 8.4:1)
pub const WEAK_TEXT: Color32 = Color32::from_rgb(0x4A, 0x4A, 0x4A);

// Glyphs (single source of truth — used in headings, dots, badges).
pub const DOT_FILLED: &str = "●";
pub const DOT_HOLLOW: &str = "○";
pub const DOT_SMALL: &str = "•";

/// Install Hive's tuned egui visuals on the given context. Called once from
/// `HiveApp::new`. We start from `Visuals::light()` (egui auto-detects dark
/// mode on some systems and the chromatic palette above is tuned for light)
/// and bump `weak_text_color` so `.weak()` labels hit AA contrast.
pub fn apply(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::light();
    // egui doesn't expose a direct `weak_text_color` setter — it's derived
    // from `widgets.noninteractive.fg_stroke.color` blended with the bg. The
    // straightforward path is to set the noninteractive fg color directly so
    // both `.weak()` (alpha-blended) and plain noninteractive labels darken
    // together.
    visuals.widgets.noninteractive.fg_stroke.color = WEAK_TEXT;
    // The `override_text_color` lets us set the primary body text explicitly;
    // leaving it None keeps egui's default near-black which is fine on white.
    ctx.set_visuals(visuals);
}
