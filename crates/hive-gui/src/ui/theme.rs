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

// Very faint cool-gray tint used to distinguish the primary worktree of a
// repo (the directory whose path matches the configured repo path) from
// linked worktrees. Premultiplied alpha keeps it readable against the
// card's grouped background.
pub const PRIMARY_WORKTREE_TINT: Color32 = Color32::from_rgba_premultiplied(60, 95, 145, 16);

// Glyph icons — painted directly via `Painter` so they don't depend on which
// Unicode blocks egui's default fonts (Ubuntu-Light / NotoEmoji / emoji-icon-font)
// happen to cover. The earlier `●▸▾↑↓✕•○` set rendered as empty squares on a
// stock install because those geometric/arrow code points are missing from all
// three default fonts. Painting via convex_polygon / circle_filled has the same
// visual weight, costs nothing, and works regardless of font configuration.

const ICON_SIZE: f32 = 14.0;
const DOT_RADIUS: f32 = 4.5;
const DOT_RADIUS_SMALL: f32 = 2.5;
/// Pixel gap between layered dots when count > 1.
const DOT_STACK_OFFSET: f32 = 4.0;
/// How many dots to render before we just stop adding more (the count badge
/// to the right of the row carries the exact number).
const MAX_STACK_DOTS: usize = 3;
/// Frame interval for the pulse animation. 30 fps is enough for a slow sine
/// fade and keeps CPU use unnoticeable.
const PULSE_FRAME_MS: u64 = 33;
/// Full pulse cycle in seconds (one trip from dim → bright → dim).
const PULSE_PERIOD_SECS: f64 = 2.0;

/// Filled circle indicator — static, single dot. For idle / classifier badges.
/// Returns the `Response` so callers can attach `.on_hover_text(...)`.
pub fn painted_dot(ui: &mut egui::Ui, color: Color32) -> egui::Response {
    let (rect, resp) =
        ui.allocate_exact_size(egui::vec2(ICON_SIZE, ICON_SIZE), egui::Sense::hover());
    ui.painter().circle_filled(rect.center(), DOT_RADIUS, color);
    resp
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
    ui.painter()
        .circle_filled(rect.center(), DOT_RADIUS_SMALL, color);
}

/// "Live" indicator — pulses alpha on a slow sine wave and stacks up to
/// `MAX_STACK_DOTS` dots when `count > 1`, with each successive dot rendered
/// at lower opacity so the depth reads even with similar hues. Callers pass
/// the *number of listeners* attributed to the row; passing 0 just paints a
/// static dot (useful when the caller hasn't branched yet).
pub fn painted_dot_pulse(ui: &mut egui::Ui, color: Color32, count: usize) {
    paint_pulsing_dots(ui, color, count, DOT_RADIUS, ICON_SIZE);
}

/// Smaller-radius pulse variant for the sidebar's nested worktree leaves.
pub fn painted_dot_small_pulse(ui: &mut egui::Ui, color: Color32, count: usize) {
    paint_pulsing_dots(ui, color, count, DOT_RADIUS_SMALL, 12.0);
}

fn paint_pulsing_dots(
    ui: &mut egui::Ui,
    color: Color32,
    count: usize,
    radius: f32,
    base_size: f32,
) {
    let dots = count.clamp(1, MAX_STACK_DOTS);
    let extra = (dots as f32 - 1.0) * DOT_STACK_OFFSET;
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(base_size + extra, base_size),
        egui::Sense::hover(),
    );

    // 2-second sine cycle. `pulse` rides 0..1; we shape it to 0.55..1.0 so the
    // dot dims but never disappears — a fully-off dot reads as "broken".
    let t = ui.input(|i| i.time);
    let raw = (t * std::f64::consts::TAU / PULSE_PERIOD_SECS).sin();
    let pulse = (raw * 0.5 + 0.5) as f32; // 0..1
    let intensity = 0.55 + pulse * 0.45; // 0.55..1.0

    let center_y = rect.center().y;
    let leftmost_x = rect.left() + base_size / 2.0;
    let painter = ui.painter();
    // Draw back-to-front so the rightmost dot (frontmost) lands on top.
    for layer in (0..dots).rev() {
        // Each layer behind the front is rendered at lower opacity.
        let layer_alpha = match layer {
            0 => 1.0,
            1 => 0.70,
            _ => 0.50,
        };
        let combined = intensity * layer_alpha;
        let c = scale_alpha(color, combined);
        painter.circle_filled(
            egui::pos2(leftmost_x + (layer as f32) * DOT_STACK_OFFSET, center_y),
            radius,
            c,
        );
    }

    // Drive the next frame so the pulse keeps animating.
    ui.ctx()
        .request_repaint_after(std::time::Duration::from_millis(PULSE_FRAME_MS));
}

/// Multiply a Color32's alpha channel by `factor` (clamped to 0..=1).
fn scale_alpha(c: Color32, factor: f32) -> Color32 {
    let f = factor.clamp(0.0, 1.0);
    let a = (c.a() as f32 * f).round() as u8;
    Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), a)
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
