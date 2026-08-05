//! All semantic colors, embedded fonts, and a handful of glyph constants
//! used across the GUI — the single source of truth `theme.rs` has always
//! been, now serving **two** named palettes (task-14):
//!
//! - **Flight Strips** (light, direction B) — the default. Board `#DFE3E6`,
//!   strip/card `#FBFBF9`, ink `#20262B`, overdue `#B3391F`.
//! - **Operator's Console** (dark, direction A's lamp language) — chassis
//!   `#221F1B`, lamp amber `#E8B04B`, jade `#57C26A`, hot `#E06C4F`.
//!
//! `ThemeChoice` (persisted on `Config::ui.theme`) selects which one is
//! active; [`apply`] installs it into egui's `Visuals` and a thread-local so
//! every `theme::green()`-style accessor below resolves to the right
//! palette without threading a parameter through every render function.
//! The thread-local is safe because all rendering happens on egui's single
//! UI thread — the same assumption `HiveApp`'s per-view `Status`/`Kick`
//! wiring already makes.
//!
//! Contrast targets — every chromatic accessor below hits **WCAG AA
//! (≥4.5:1)** against its own theme's panel background. `legibility_audit`
//! (in `tests/`) proves this by rendering real views under both themes and
//! walking the actual painted draw list, not by trusting the doc comments.
//!
//! Secondary ("muted") text uses [`muted_text`], *not* egui's `.weak()`.
//! `.weak()` resolves to `gray_out(text_color)`, which blends the text 50%
//! toward the panel — landing at ~2.4:1 no matter how dark the base color is
//! (the blend target is the panel, so AA is mathematically unreachable).
//!
//! `apply(ctx, choice)` is cheap (a `Visuals` struct swap + one style entry)
//! and safe to call every frame — production does, so a live toggle takes
//! effect immediately. Embedded fonts are the expensive part (font-atlas
//! rebuild); [`install_fonts`] is separate and called exactly once, from
//! `HiveApp::new`.

use super::legibility;
use eframe::egui::{self, Color32};
use std::cell::Cell;
pub use switchbard_core::config::ThemeChoice;

/// One theme's full set of semantic colors. Every field has a light
/// ([`LIGHT`]) and dark ([`DARK`]) value below; nothing in the rest of the
/// GUI ever constructs a `Color32` for these roles directly.
struct Palette {
    /// Healthy / running / has-listeners.
    green: Color32,
    /// Dirty / ambiguous classifier verdict.
    amber: Color32,
    /// Soft amber for the Servers classifier "Maybe" dot — same luminance
    /// band as `amber` but a warmer hue, so it reads as a question.
    amber_question: Color32,
    /// Ahead/behind drifted from origin.
    lavender: Color32,
    /// External-live: bound but not by us. Also the hyperlink color.
    sky: Color32,
    /// Blocked / port-conflict warning; the design mock's "hot" line color.
    warn_orange: Color32,
    /// Destructive-action **text** color (validation errors, inline warnings
    /// via `.colored_label`). Deliberately a separate role from the danger
    /// **button fill** in [`danger_button`]: a text color on a themed panel
    /// needs to be legible *against that panel*, while a button fill with
    /// white text on top needs to stay dark enough for *that* contrast pair
    /// — on a dark chassis those two constraints point in opposite
    /// directions (bright red reads on `#221F1B`; white-on-bright-red does
    /// not clear AA). `danger_button` uses the theme-independent
    /// `danger_fill()` instead of this field for exactly that reason.
    danger: Color32,
    /// Primary body text.
    weak_text: Color32,
    /// Secondary / de-emphasized text (paths, hints, counts, separators).
    muted_text: Color32,
    /// The "highlighter on a page" tint marking a repo's primary worktree.
    primary_worktree_tint: Color32,
    /// The window/central-panel background — Flight Strips' "board" /
    /// Operator's Console's "chassis".
    panel_fill: Color32,
    /// Recessed background (kanban column bodies, code/notes blocks).
    faint_bg: Color32,
    /// Raised background (flight-strip cards, the markdown description
    /// frame) — Flight Strips' "strip" / Operator's Console's lit panel.
    extreme_bg: Color32,
}

// Flight Strips (light, direction B) — board #DFE3E6, strip #FBFBF9,
// overdue #B3391F. The board panel (L≈0.76) is *darker* than egui's stock
// light panel (L≈0.91) the pre-task-14 palette was tuned against, so most
// chromatic constants needed re-darkening to hold WCAG AA on the new ground
// — exactly the "retune the AA constants per ground" task-14 called for.
// Ratios below are against `panel_fill`, computed via `legibility::contrast_
// ratio`/`relative_luminance` and confirmed green by `legibility_audit`.
const LIGHT: Palette = Palette {
    green: Color32::from_rgb(0x0E, 0x66, 0x2B), // ~5.5:1
    amber: Color32::from_rgb(0x7A, 0x50, 0x00), // ~5.5:1
    amber_question: Color32::from_rgb(0x8A, 0x52, 0x00), // ~5.0:1
    lavender: Color32::from_rgb(0x3F, 0x3F, 0xB0), // ~6.4:1 (unchanged, already ample)
    sky: Color32::from_rgb(0x15, 0x5A, 0x8A),   // ~5.7:1; matches the mock's dispatch blue
    warn_orange: Color32::from_rgb(0xB3, 0x39, 0x1F), // design's "overdue" red, ~4.6:1
    danger: Color32::from_rgb(0xA8, 0x36, 0x36), // text role, ~5.0:1 (button fill is danger_fill())
    weak_text: Color32::from_rgb(0x20, 0x26, 0x2B), // design's "ink"
    muted_text: Color32::from_rgb(0x50, 0x5A, 0x63), // ~5.5:1
    primary_worktree_tint: Color32::from_rgba_premultiplied(28, 25, 11, 28),
    panel_fill: Color32::from_rgb(0xDF, 0xE3, 0xE6), // "board"
    faint_bg: Color32::from_rgb(0xE7, 0xEA, 0xEC),   // column recess, between board/strip
    extreme_bg: Color32::from_rgb(0xFB, 0xFB, 0xF9), // "strip"
};

// Operator's Console (dark, direction A) — chassis #221F1B, lamp amber
// #E8B04B, jade #57C26A, hot #E06C4F, faceplate text #D8D2C6. Light-on-dark
// text needs the *opposite* tuning direction from the light theme (bright
// enough, not dark enough); `warn_orange` is brightened past the mock's
// literal #E06C4F because that exact hex clears AA only against the darker
// `panel_fill`, not the slightly lighter `extreme_bg` cards it also renders
// on. `danger` here is a bright hot-orange for the same reason `danger` in
// LIGHT isn't reused for `danger_button`'s fill — see the field doc.
const DARK: Palette = Palette {
    green: Color32::from_rgb(0x57, 0xC2, 0x6A), // lamp jade, ~6.6:1
    amber: Color32::from_rgb(0xE8, 0xB0, 0x4B), // lamp amber, ~7.6:1
    amber_question: Color32::from_rgb(0xE8, 0xB0, 0x4B), // one lamp amber, dark side
    lavender: Color32::from_rgb(0x9E, 0x9E, 0xFF), // ~6.2:1
    sky: Color32::from_rgb(0x6F, 0xB8, 0xE8),   // ~6.9:1
    warn_orange: Color32::from_rgb(0xE8, 0x7A, 0x5A), // brightened "line hot", ~5.2:1
    danger: Color32::from_rgb(0xE8, 0x7A, 0x5A), // text role; button fill is danger_fill()
    weak_text: Color32::from_rgb(0xD8, 0xD2, 0xC6), // faceplate text, ~9.9:1
    muted_text: Color32::from_rgb(0xAD, 0xA6, 0x97), // ~6.1:1
    primary_worktree_tint: Color32::from_rgba_premultiplied(28, 25, 11, 28),
    panel_fill: Color32::from_rgb(0x22, 0x1F, 0x1B), // chassis
    faint_bg: Color32::from_rgb(0x1C, 0x1A, 0x17),   // recessed
    extreme_bg: Color32::from_rgb(0x2B, 0x27, 0x21), // lit panel / selected row
};

fn palette_for(choice: ThemeChoice) -> &'static Palette {
    match choice {
        ThemeChoice::Light => &LIGHT,
        ThemeChoice::Dark => &DARK,
    }
}

thread_local! {
    /// The theme every `theme::xxx()` accessor below resolves against for
    /// the current frame. All egui rendering happens on one UI thread, so a
    /// thread-local (rather than threading a `Palette` parameter through
    /// every render function in `ui/**`) is sound and keeps every existing
    /// `theme::green()`-style call site a plain zero-argument call.
    static ACTIVE: Cell<ThemeChoice> = const { Cell::new(ThemeChoice::Light) };
}

fn active_palette() -> &'static Palette {
    palette_for(ACTIVE.with(Cell::get))
}

pub fn green() -> Color32 {
    active_palette().green
}
pub fn amber() -> Color32 {
    active_palette().amber
}
pub fn amber_question() -> Color32 {
    active_palette().amber_question
}
pub fn lavender() -> Color32 {
    active_palette().lavender
}
pub fn sky() -> Color32 {
    active_palette().sky
}
pub fn warn_orange() -> Color32 {
    active_palette().warn_orange
}
pub fn danger() -> Color32 {
    active_palette().danger
}
pub fn weak_text() -> Color32 {
    active_palette().weak_text
}
pub fn muted_text() -> Color32 {
    active_palette().muted_text
}
pub fn primary_worktree_tint() -> Color32 {
    active_palette().primary_worktree_tint
}
/// Recessed background (kanban column bodies, code/notes blocks). See
/// `board::render_column` for why the Board lens can't just rely on
/// `ui.visuals().faint_bg_color` at its call site.
pub fn faint_bg() -> Color32 {
    active_palette().faint_bg
}

// Glyph icons — painted directly via `Painter` so they don't depend on which
// Unicode blocks the installed fonts happen to cover. The earlier
// `●▸▾↑↓✕•○` set rendered as empty squares on a stock install because those
// geometric/arrow code points are missing from all three default fonts.
// Painting via convex_polygon / circle_filled has the same visual weight,
// costs nothing, and works regardless of font configuration.

const ICON_SIZE: f32 = 14.0;
const DOT_RADIUS: f32 = 4.5;
const DOT_RADIUS_SMALL: f32 = 2.5;
/// Pixel gap between layered dots when count > 1.
const DOT_STACK_OFFSET: f32 = 4.0;
/// How many dots to render before we just stop adding more (the count badge
/// to the right of the row carries the exact number).
const MAX_STACK_DOTS: usize = 3;
/// Frame interval for the pulse animation. The pulse is decorative, so keep it
/// intentionally low-rate to avoid turning an idle dashboard into a repaint loop.
const PULSE_FRAME_MS: u64 = 500;
/// Full pulse cycle in seconds (one trip from dim → bright → dim).
const PULSE_PERIOD_SECS: f64 = 2.0;

/// The destructive-button fill (Kill, Stop, Confirm, Archive) paired with
/// explicit white text — deliberately **not** theme-switched like every
/// other color in this file. White-on-fill contrast only depends on the fill
/// itself, never on the surrounding panel, so one dark red clears AA in both
/// themes; see [`danger`][fn@danger]'s doc for why that same red can't also
/// serve as dark theme's *text*-colored danger.
fn danger_fill() -> Color32 {
    Color32::from_rgb(0xB4, 0x3C, 0x3C) // ~5.7:1 with white text
}

/// Destructive button (Kill, Stop, Confirm, Archive) — `danger_fill()` plus
/// explicit white text. The default button text color is dark, which only
/// reaches ~2.1:1 contrast against that fill. This helper centralizes both
/// decisions so every call site stays consistent.
pub fn danger_button(text: &str) -> egui::Button<'static> {
    egui::Button::new(egui::RichText::new(text.to_string()).color(Color32::WHITE))
        .fill(danger_fill())
}

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

/// Seven distinguishable hues for the Board lens's repo rails / badges — a
/// tracked worktree's fleet is small (task-15 targets ~7 repos), so a fixed
/// palette indexed by a stable hash reads more consistently frame-to-frame
/// than deriving hue from an arbitrary HSV wheel. Not used for text, so the
/// WCAG-AA text contract in this module's header doc doesn't apply — these
/// only ever paint a dot or a rail, never a glyph. Same seven hues in both
/// themes (they read fine on both a light board and a dark chassis).
const REPO_RAIL_COLORS: [Color32; 7] = [
    Color32::from_rgb(0x15, 0x5A, 0x8A), // budget-style blue
    Color32::from_rgb(0x7A, 0x4A, 0x9E), // music-style violet
    Color32::from_rgb(0x0F, 0x7A, 0x4D), // kitchen-style green
    Color32::from_rgb(0xB3, 0x65, 0x1F), // builtin-style amber
    Color32::from_rgb(0x8A, 0x2F, 0x5D), // onramp-style magenta
    Color32::from_rgb(0x40, 0x47, 0x4D), // hub-style slate
    Color32::from_rgb(0x1F, 0x8A, 0x8A), // teal, the 7th distinguishable hue
];

/// Stable repo → rail-color mapping. Same repo name always paints the same
/// hue across rows, columns, and frames (a `str` hash, not an index into
/// discovery order, so adding/removing a tracked repo doesn't reshuffle
/// everyone else's color).
pub fn repo_rail_color(repo_name: &str) -> Color32 {
    let mut hash: u64 = 0xcbf29ce484222325; // FNV-1a offset basis
    for byte in repo_name.as_bytes() {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    REPO_RAIL_COLORS[(hash as usize) % REPO_RAIL_COLORS.len()]
}

/// Multiply a Color32's alpha channel by `factor` (clamped to 0..=1).
fn scale_alpha(c: Color32, factor: f32) -> Color32 {
    let f = factor.clamp(0.0, 1.0);
    let a = (c.a() as f32 * f).round() as u8;
    Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), a)
}

/// Small trash-can icon used as a destructive row-action affordance (e.g.
/// "remove worktree"). Painter-drawn for the same reason as the other glyphs
/// in this file: stock fonts don't cover icon code points, so a literal
/// `🗑` or `✕` renders as a tofu square.
///
/// Reads `resp.hovered()` before painting so the icon picks up `danger()`
/// on hover and `weak_text()` at rest — enough visual signal that this is a
/// destructive action without screaming for attention on every row.
pub fn painted_trash_button(ui: &mut egui::Ui) -> egui::Response {
    let (rect, resp) =
        ui.allocate_exact_size(egui::vec2(ICON_SIZE, ICON_SIZE), egui::Sense::click());
    let color = if resp.hovered() {
        danger()
    } else {
        weak_text()
    };
    let stroke = egui::Stroke::new(1.4, color);
    let painter = ui.painter();
    let c = rect.center();
    // Handle (small horizontal cap above the lid).
    painter.line_segment(
        [
            egui::pos2(c.x - 1.5, c.y - 5.0),
            egui::pos2(c.x + 1.5, c.y - 5.0),
        ],
        stroke,
    );
    // Lid (wider horizontal line).
    painter.line_segment(
        [
            egui::pos2(c.x - 4.0, c.y - 3.0),
            egui::pos2(c.x + 4.0, c.y - 3.0),
        ],
        stroke,
    );
    // Body — slight trapezoid: narrower at the bottom for the classic can shape.
    painter.line_segment(
        [
            egui::pos2(c.x - 3.0, c.y - 2.0),
            egui::pos2(c.x - 2.5, c.y + 4.0),
        ],
        stroke,
    );
    painter.line_segment(
        [
            egui::pos2(c.x + 3.0, c.y - 2.0),
            egui::pos2(c.x + 2.5, c.y + 4.0),
        ],
        stroke,
    );
    painter.line_segment(
        [
            egui::pos2(c.x - 2.5, c.y + 4.0),
            egui::pos2(c.x + 2.5, c.y + 4.0),
        ],
        stroke,
    );
    resp
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

// Embedded OFL fonts (task-14): Barlow Semi Condensed for labels (DIN/
// aviation lineage, matching Flight Strips' ATC metaphor), JetBrains Mono
// for ids/numerics. License text ships alongside in `assets/fonts/*/OFL.txt`.
// Registered ahead of egui's bundled defaults, which remain as fallbacks for
// any glyph these two don't cover (emoji, non-Latin scripts).
const BARLOW_SEMI_CONDENSED: &[u8] =
    include_bytes!("../../assets/fonts/barlow-semi-condensed/BarlowSemiCondensed-Regular.ttf");
const JETBRAINS_MONO: &[u8] = include_bytes!("../../assets/fonts/jetbrains-mono/JetBrainsMono.ttf");

/// Install the embedded fonts into `ctx`'s font atlas. Expensive (rebuilds
/// the atlas), so call exactly once — from `HiveApp::new` — never per frame.
pub fn install_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(
        "barlow_semi_condensed".to_owned(),
        egui::FontData::from_static(BARLOW_SEMI_CONDENSED).into(),
    );
    fonts.font_data.insert(
        "jetbrains_mono".to_owned(),
        egui::FontData::from_static(JETBRAINS_MONO).into(),
    );
    fonts
        .families
        .entry(egui::FontFamily::Proportional)
        .or_default()
        .insert(0, "barlow_semi_condensed".to_owned());
    fonts
        .families
        .entry(egui::FontFamily::Monospace)
        .or_default()
        .insert(0, "jetbrains_mono".to_owned());
    ctx.set_fonts(fonts);
}

/// Install Switchbard's tuned egui visuals for `choice` on the given
/// context, and make every `theme::xxx()` accessor in this module resolve
/// against it for the rest of this frame. Cheap (`Visuals` swap + one style
/// entry) — safe, and necessary, to call every frame so a live theme toggle
/// takes effect immediately.
pub fn apply(ctx: &egui::Context, choice: ThemeChoice) {
    ACTIVE.with(|cell| cell.set(choice));
    let palette = palette_for(choice);

    let mut visuals = match choice {
        ThemeChoice::Light => egui::Visuals::light(),
        ThemeChoice::Dark => egui::Visuals::dark(),
    };
    // Body text. `text_color()` reads `widgets.noninteractive.fg_stroke.color`,
    // so setting it here makes plain labels paint with `weak_text()`.
    visuals.widgets.noninteractive.fg_stroke.color = palette.weak_text;
    // `sky()` doubles as the hyperlink color in both themes — egui's own
    // default (light theme: #009BFF, 2.8:1) doesn't clear AA on a light panel.
    visuals.hyperlink_color = palette.sky;
    visuals.panel_fill = palette.panel_fill;
    visuals.window_fill = palette.panel_fill;
    visuals.faint_bg_color = palette.faint_bg;
    visuals.extreme_bg_color = palette.extreme_bg;
    // `TextEdit` hint text and `Ui::disable()` both fade a color 50% toward
    // `Visuals::fade_out_to_color()`, which reads exactly this one field
    // (`widgets.noninteractive.weak_bg_fill` — no other visible widget uses
    // it, so this is a safe, narrowly-scoped override). Left at egui's
    // stock default it blends toward a near-panel gray, which is how the
    // stock light theme's `.weak()`/hint-text path this file's own header
    // already documents as "AA mathematically unreachable" happens. Blending
    // toward `muted_text` instead of a panel-ish gray keeps hint text (which
    // — unlike `Ui::disable()`'s targets, exempt under WCAG 1.4.3 — the user
    // is meant to read) inside the same tuned, AA-safe tonal range.
    visuals.widgets.noninteractive.weak_bg_fill = palette.muted_text;
    ctx.set_visuals(visuals);

    // egui ships `Small` at 9pt — below the legibility floor. Raise it to the
    // floor so every `.small()` call site clears the size contract in one move,
    // while staying a step below `Body` (12.5pt) so the hierarchy survives.
    ctx.style_mut(|style| {
        style.text_styles.insert(
            egui::TextStyle::Small,
            egui::FontId::proportional(legibility::MIN_FONT_POINTS),
        );
    });
}
