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

// Glyph icons — painted directly via `Painter` so they don't depend on which
// Unicode blocks egui's default fonts (Ubuntu-Light / NotoEmoji / emoji-icon-font)
// happen to cover. The earlier `●▸▾↑↓✕•○` set rendered as empty squares on a
// stock install because those geometric/arrow code points are missing from all
// three default fonts. Painting via convex_polygon / circle_filled has the same
// visual weight, costs nothing, and works regardless of font configuration.

const ICON_SIZE: f32 = 14.0;

/// Filled circle indicator (active / has-listeners / repo-with-services).
pub fn painted_dot(ui: &mut egui::Ui, color: Color32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(ICON_SIZE, ICON_SIZE), egui::Sense::hover());
    ui.painter().circle_filled(rect.center(), 4.5, color);
}

/// Hollow circle indicator (used for the "Unattributed" listener section).
pub fn painted_dot_hollow(ui: &mut egui::Ui, color: Color32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(ICON_SIZE, ICON_SIZE), egui::Sense::hover());
    ui.painter()
        .circle_stroke(rect.center(), 4.0, egui::Stroke::new(1.5, color));
}

/// Smaller filled circle for nested rows (worktree leaves in the sidebar tree).
pub fn painted_dot_small(ui: &mut egui::Ui, color: Color32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(12.0, 12.0), egui::Sense::hover());
    ui.painter().circle_filled(rect.center(), 2.5, color);
}

/// Expand / collapse caret. Triangle points down when `open`, right when not.
/// Returns the click response so callers can toggle their state on click.
pub fn caret_button(ui: &mut egui::Ui, open: bool) -> egui::Response {
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(ICON_SIZE, ICON_SIZE), egui::Sense::click());
    let color = ui.visuals().text_color();
    let c = rect.center();
    let pts = if open {
        vec![
            egui::pos2(c.x - 3.5, c.y - 2.0),
            egui::pos2(c.x + 3.5, c.y - 2.0),
            egui::pos2(c.x, c.y + 2.5),
        ]
    } else {
        vec![
            egui::pos2(c.x - 2.0, c.y - 3.5),
            egui::pos2(c.x - 2.0, c.y + 3.5),
            egui::pos2(c.x + 2.5, c.y),
        ]
    };
    ui.painter()
        .add(egui::Shape::convex_polygon(pts, color, egui::Stroke::NONE));
    response
}

/// Compact triangle button (up or down). Disabled state renders weaker
/// and consumes hover but no clicks.
pub fn triangle_button(ui: &mut egui::Ui, up: bool, enabled: bool) -> egui::Response {
    let sense = if enabled {
        egui::Sense::click()
    } else {
        egui::Sense::hover()
    };
    let (rect, response) = ui.allocate_exact_size(egui::vec2(ICON_SIZE, ICON_SIZE), sense);
    let color = if !enabled {
        ui.visuals().weak_text_color()
    } else if response.hovered() {
        ui.visuals().strong_text_color()
    } else {
        ui.visuals().text_color()
    };
    let c = rect.center();
    let pts = if up {
        vec![
            egui::pos2(c.x, c.y - 3.0),
            egui::pos2(c.x - 3.5, c.y + 2.5),
            egui::pos2(c.x + 3.5, c.y + 2.5),
        ]
    } else {
        vec![
            egui::pos2(c.x, c.y + 3.0),
            egui::pos2(c.x - 3.5, c.y - 2.5),
            egui::pos2(c.x + 3.5, c.y - 2.5),
        ]
    };
    ui.painter()
        .add(egui::Shape::convex_polygon(pts, color, egui::Stroke::NONE));
    response
}

/// X-shape glyph painted as two crossed strokes — used in the Servers view's
/// "doesn't look like a server" classifier badge.
pub fn painted_x(ui: &mut egui::Ui, color: Color32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(ICON_SIZE, ICON_SIZE), egui::Sense::hover());
    let c = rect.center();
    let stroke = egui::Stroke::new(1.5, color);
    ui.painter().line_segment(
        [c + egui::vec2(-3.0, -3.0), c + egui::vec2(3.0, 3.0)],
        stroke,
    );
    ui.painter().line_segment(
        [c + egui::vec2(3.0, -3.0), c + egui::vec2(-3.0, 3.0)],
        stroke,
    );
}

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
