//! All semantic colors, embedded fonts, and a handful of glyph constants
//! used across the GUI — the single source of truth `theme.rs` has always
//! been, now serving **two** named palettes (task-14):
//!
//! - **Flight Strips** (light, direction B) — the default. Cool board,
//!   paper-white strips, ink `#20262B`, overdue `#B3391F`.
//! - **Operator's Console** (dark, direction A's lamp language) — blue-black
//!   chassis, lamp amber `#E8B04B`, jade `#57C26A`, hot `#E06C4F`.
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
    /// Dispatch affordance/state ("PATCH ▸" / "DISPATCH ▸" in the design
    /// mock). Unlike every other role, the two themes deliberately use
    /// *different* hues here rather than a light/dark shade of the same
    /// one: Flight Strips' dispatch action is the mock's own "send" blue
    /// (identical to `sky` — kept as its own field for the semantic name,
    /// not because the hex differs); Operator's Console borrows its lamp
    /// amber (identical to `amber`) for the same "patching a line through"
    /// language the mock uses for agent/dispatch state.
    dispatch_accent: Color32,
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
    /// Fill behind a bulk-selected worktree row. Deliberately a *cool* wash
    /// where `primary_worktree_tint` is warm, so the two never read as the
    /// same state, and low-alpha so it tints the row rather than repainting
    /// it — every label on the row still has to clear AA against the result,
    /// which `legibility_audit` measures from the real draw list.
    selected_row_tint: Color32,
    /// The outline on a bulk-selected row. The fill alone is too quiet to
    /// answer "which rows did 'Select all merged + clean' just take?" at a
    /// glance, and an outline carries that at any alpha without touching the
    /// contrast of the text inside it.
    selected_row_stroke: Color32,
    /// The window/central-panel background — Flight Strips' "board" /
    /// Operator's Console's "chassis". The nav strip (`nav_bg`) and rail
    /// (`rail_bg`) sit visually *above* this; recessed surfaces (`faint_bg`,
    /// and — via `apply()`'s `visuals.extreme_bg_color` — every text input)
    /// sit *below* it; cards (`card_bg`) sit *above* both.
    panel_fill: Color32,
    /// Recessed background: kanban column bodies, code/notes blocks, and
    /// (owner UX pass, 2026-08-05) every text input's fill — a sunken field
    /// you type *into*, the opposite metaphor from a raised card you read.
    faint_bg: Color32,
    /// Raised background — flight-strip/digest cards, the markdown
    /// description frame, the Statistics burndown chart: Flight Strips'
    /// "strip" / Operator's Console's lit panel. Named `card_bg` (not
    /// `extreme_bg`, egui's own field name for this slot) because the owner
    /// UX pass split it from input fields, which used to share this exact
    /// value via `visuals.extreme_bg_color` — the "everything feels
    /// dominated by gray" complaint traced in part to a card and a text
    /// input being visually identical. See `card_bg()`'s doc.
    card_bg: Color32,
    /// The top-bar view-tabs / lens-tabs strip's own background band —
    /// distinct from `panel_fill` so navigation reads as its own zone
    /// rather than blending into the content below it (owner UX pass).
    nav_bg: Color32,
    /// The Backlog view's persistent detail rail's own background (owner UX
    /// pass, TASK-34) — a third workspace tier alongside the board
    /// (`panel_fill`) and its cards (`card_bg`), so the rail reads as its
    /// own persistent zone rather than "more board."
    rail_bg: Color32,
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
    dispatch_accent: Color32::from_rgb(0x15, 0x5A, 0x8A), // same as sky, ~5.7:1
    warn_orange: Color32::from_rgb(0xB3, 0x39, 0x1F), // design's "overdue" red, ~4.6:1
    danger: Color32::from_rgb(0xA8, 0x36, 0x36), // text role, ~5.0:1 (button fill is danger_fill())
    weak_text: Color32::from_rgb(0x20, 0x26, 0x2B), // design's "ink"
    muted_text: Color32::from_rgb(0x50, 0x5A, 0x63), // ~5.5:1
    primary_worktree_tint: Color32::from_rgba_premultiplied(28, 25, 11, 28),
    selected_row_tint: Color32::from_rgba_premultiplied(10, 30, 46, 40),
    selected_row_stroke: Color32::from_rgb(0x15, 0x5A, 0x8A), // matches `sky`
    panel_fill: Color32::from_rgb(0xE7, 0xEC, 0xF0),          // cool workspace board
    faint_bg: Color32::from_rgb(0xD9, 0xE1, 0xE7),            // recessed controls and column wells
    card_bg: Color32::from_rgb(0xFF, 0xFF, 0xFD),             // clean paper strip
    nav_bg: Color32::from_rgb(0xF4, 0xF7, 0xF9),              // elevated navigation surface
    rail_bg: Color32::from_rgb(0xF0, 0xF3, 0xF5),             // quiet detail workspace
};

// Operator's Console (dark, direction A) — chassis #221F1B, lamp amber
// #E8B04B, jade #57C26A, hot #E06C4F, faceplate text #D8D2C6. Light-on-dark
// text needs the *opposite* tuning direction from the light theme (bright
// enough, not dark enough); `warn_orange` is brightened past the mock's
// literal #E06C4F because that exact hex clears AA only against the darker
// `panel_fill`, not the slightly lighter `card_bg` cards it also renders on.
// `danger` here is a bright hot-orange for the same reason `danger` in
// LIGHT isn't reused for `danger_button`'s fill — see the field doc.
//
// Owner UX pass (2026-08-05): the original `faint_bg`/`panel_fill`/
// `extreme_bg` trio spanned only #1C1A17 -> #221F1B -> #2B2721 — a ~9-value
// swing per channel on a 0-255 scale, barely perceptible, and the root
// cause underneath "everything feels dominated by gray" for this theme
// specifically (Flight Strips' equivalent spread is ~3x wider). Widened to
// four visually distinct tiers — recessed/input (`faint_bg`), ground
// (`panel_fill`), rail, and raised/card (`card_bg`, brightest) — each
// re-verified by `legibility_audit` against whatever it actually sits
// behind, not assumed.
const DARK: Palette = Palette {
    green: Color32::from_rgb(0x57, 0xC2, 0x6A), // lamp jade, ~6.6:1
    amber: Color32::from_rgb(0xE8, 0xB0, 0x4B), // lamp amber, ~7.6:1
    amber_question: Color32::from_rgb(0xE8, 0xB0, 0x4B), // one lamp amber, dark side
    lavender: Color32::from_rgb(0x9E, 0x9E, 0xFF), // ~6.2:1
    sky: Color32::from_rgb(0x6F, 0xB8, 0xE8),   // ~6.9:1
    dispatch_accent: Color32::from_rgb(0xE8, 0xB0, 0x4B), // same as lamp amber, ~7.6:1
    warn_orange: Color32::from_rgb(0xE8, 0x7A, 0x5A), // brightened "line hot", ~5.2:1
    danger: Color32::from_rgb(0xE8, 0x7A, 0x5A), // text role; button fill is danger_fill()
    weak_text: Color32::from_rgb(0xE6, 0xE1, 0xD8), // crisp faceplate text
    muted_text: Color32::from_rgb(0xB8, 0xB1, 0xA4), // readable secondary faceplate text
    primary_worktree_tint: Color32::from_rgba_premultiplied(28, 25, 11, 28),
    selected_row_tint: Color32::from_rgba_premultiplied(20, 42, 60, 52),
    selected_row_stroke: Color32::from_rgb(0x6F, 0xB8, 0xE8), // matches `sky`
    // Surface values are spread as far as the AA contract allows, not packed
    // into a narrow band. The previous set squeezed all five into luminance
    // 0.006-0.022 — rail vs panel measured 1.02:1, i.e. the same colour —
    // which left `surface_stroke` doing all the separation work and read as
    // lines drawn on a flat sheet. `card_bg` is the binding constraint: any
    // lighter and `danger` text on a card drops below the 4.5:1 body floor
    // (it sits at 4.52:1 here). See `legibility_audit`.
    panel_fill: Color32::from_rgb(0x1B, 0x20, 0x28), // blue-black chassis
    faint_bg: Color32::from_rgb(0x0A, 0x0D, 0x10),   // recessed controls and column wells
    card_bg: Color32::from_rgb(0x2B, 0x32, 0x3E),    // raised instrument panel
    nav_bg: Color32::from_rgb(0x25, 0x2C, 0x36),     // elevated navigation surface
    rail_bg: Color32::from_rgb(0x11, 0x15, 0x1A),    // quiet detail workspace
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
pub fn dispatch_accent() -> Color32 {
    active_palette().dispatch_accent
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

pub fn selected_row_tint() -> Color32 {
    active_palette().selected_row_tint
}

pub fn selected_row_stroke() -> egui::Stroke {
    egui::Stroke::new(1.0, active_palette().selected_row_stroke)
}
/// Recessed background (kanban column bodies, code/notes blocks, and every
/// text input's fill via `apply()`'s `visuals.extreme_bg_color`). See
/// `board::render_column` for why the Board lens can't just rely on
/// `ui.visuals().faint_bg_color` at its call site.
pub fn faint_bg() -> Color32 {
    active_palette().faint_bg
}
/// Raised card surface (flight-strip/digest cards, the Statistics burndown
/// chart) — deliberately its own accessor, not `ui.visuals().extreme_bg_
/// color`, since the owner UX pass repointed that egui slot to `faint_bg`
/// for input fields instead. Call sites that painted a card via `ui.visuals
/// ().extreme_bg_color` before this pass should read this instead.
pub fn card_bg() -> Color32 {
    active_palette().card_bg
}
/// The top-bar view-tabs / lens-tabs strip's own background band (owner UX
/// pass) — wrap that row in a `Frame::default().fill(theme::nav_bg())` so
/// navigation reads as its own zone.
pub fn nav_bg() -> Color32 {
    active_palette().nav_bg
}
/// The Backlog view's persistent detail rail's own background (owner UX
/// pass, TASK-34's rail) — a third workspace tier alongside the board and
/// its cards.
pub fn rail_bg() -> Color32 {
    active_palette().rail_bg
}

/// Hairline used to separate adjacent workspace surfaces without reverting
/// to egui's heavy stock-gray borders.
pub fn surface_stroke() -> egui::Stroke {
    egui::Stroke::new(1.0, scale_alpha(weak_text(), 0.40))
}

/// A shadow for raised content cards.
///
/// Deliberately stronger than the surface-value difference alone. Two large
/// adjacent backgrounds (the detail rail against the board, say) cannot
/// separate on value at AA-safe levels — the ceiling imposed by `card_bg` is
/// only ~1.5:1 against its own well — so the edge has to carry what the fill
/// cannot. Hierarchy still comes from value first; this is what makes the
/// value difference legible rather than a substitute for it.
pub fn card_shadow() -> egui::epaint::Shadow {
    egui::epaint::Shadow {
        offset: [0, 3],
        blur: 10,
        spread: 0,
        color: Color32::from_black_alpha(72),
    }
}
/// Board lens: a drop-target column's fill while a drag is hovering over it
/// *and* egui's `dnd_drop_zone` would actually accept the drop (task-42).
/// `dnd_drop_zone` swaps in `visuals().widgets.active` for exactly that
/// condition (`is_anything_being_dragged && can_accept_what_is_being_dragged
/// && response.contains_pointer()` — see its own source) and `.inactive`
/// otherwise; `board::render_column` used to point *both* slots at the same
/// `faint_bg()`, which is why a drag hovering a column produced no visible
/// feedback at all. Reuses `sky()` — this app's one "interactive accent" hue
/// (selection stroke, hyperlinks) — at low alpha rather than minting a new
/// hue: the semantic here ("you're about to interact with this") is the
/// same one `sky()` already carries everywhere else it's used.
pub fn drop_target_fill() -> Color32 {
    scale_alpha(sky(), 0.22)
}
/// Pairs with [`drop_target_fill`] for the same hovered-drop-target column's
/// border.
pub fn drop_target_stroke() -> egui::Stroke {
    egui::Stroke::new(2.0, sky())
}
/// "Idle" / "no activity" indicator dot — owner UX pass (2026-08-05):
/// centralizes what was six duplicated `egui::Color32::GRAY` call sites
/// (sidebar.rs, workspace/mod.rs, agent_context.rs). A flat, untethered
/// gray doesn't shift with the active theme the way every other semantic
/// color in this file does, which is part of what "everything feels
/// dominated by gray" was naming — this reuses `muted_text()`, already
/// AA-tuned per theme, since "idle" and "de-emphasized" are the same
/// semantic here.
pub fn idle_dot() -> Color32 {
    muted_text()
}

// Glyph icons — painted directly via `Painter` so they don't depend on which
// Unicode blocks the installed fonts happen to cover. The earlier
// `●▸▾↑↓✕•○` set rendered as empty squares on a stock install because those
// geometric/arrow code points are missing from all three default fonts.
// Painting via convex_polygon / circle_filled has the same visual weight,
// costs nothing, and works regardless of font configuration.

pub const ICON_SIZE: f32 = 14.0;
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

/// The affirmative-button fill (onboarding's "Add selected" / "Browse for a
/// folder…"), theme-independent for the same reason as `danger_fill()`:
/// white-on-fill contrast depends only on the fill. Found *while* writing
/// `success_button()` (owner UX pass, 2026-08-05), not before: the
/// call sites this replaces used `theme::green()` directly as a button
/// fill — Operator's Console's `green()` is a bright lamp-jade (~6.6:1 as
/// *text* against the dark chassis), which does not clear AA with *white
/// text on top of it*, the same theme-vs-button-fill conflict `danger`
/// already had to solve. No fixture exercised the onboarding overlay to
/// catch this before now — one added below.
fn success_fill() -> Color32 {
    Color32::from_rgb(0x0E, 0x66, 0x2B) // Flight Strips' own green(), ~5.5:1 with white text
}

/// Destructive button (Kill, Stop, Confirm, Archive) — `danger_fill()` plus
/// explicit white text. The default button text color is dark, which only
/// reaches ~2.1:1 contrast against that fill. This helper centralizes both
/// decisions so every call site stays consistent.
pub fn danger_button(text: &str) -> egui::Button<'static> {
    egui::Button::new(egui::RichText::new(text.to_string()).color(Color32::WHITE))
        .fill(danger_fill())
}

/// Affirmative/primary-action button (onboarding's "Add selected", "Browse
/// for a folder…") — `success_fill()` plus explicit white text, the same
/// "white text needs a fill dark enough for *that* pair, independent of
/// theme" reasoning `danger_button` documents. Owner UX pass (2026-08-05):
/// centralizes what was two duplicated `Button::new(...).fill(theme::green
/// ())` call sites in onboarding.rs — which, per `success_fill()`'s own
/// doc, were an actual (previously untested) AA failure in dark mode, not
/// just a DRY nit.
pub fn success_button(text: &str) -> egui::Button<'static> {
    egui::Button::new(egui::RichText::new(text.to_string()).color(Color32::WHITE))
        .fill(success_fill())
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
/// `pub(crate)` (not private) so `board::paint_card`'s task-42 landing-flash
/// fade can reuse it instead of re-deriving the same alpha math locally.
pub(crate) fn scale_alpha(c: Color32, factor: f32) -> Color32 {
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

/// Small pencil icon — the universal "edit" affordance (e.g. a goal's weekly
/// target, Goals place TASK-101). Painter-drawn for the same font-coverage
/// reason as every other icon in this file.
pub fn painted_pencil_button(ui: &mut egui::Ui) -> egui::Response {
    let (rect, resp) =
        ui.allocate_exact_size(egui::vec2(ICON_SIZE, ICON_SIZE), egui::Sense::click());
    let color = if resp.hovered() { sky() } else { weak_text() };
    let stroke = egui::Stroke::new(1.3, color);
    let painter = ui.painter();
    let c = rect.center();
    // Shaft: a diagonal stroke from the tip to the eraser end.
    painter.line_segment(
        [
            egui::pos2(c.x - 4.5, c.y + 4.5),
            egui::pos2(c.x + 3.5, c.y - 3.5),
        ],
        stroke,
    );
    // Tip: a short "V" closing the shaft's bottom-left end.
    painter.line_segment(
        [
            egui::pos2(c.x - 4.5, c.y + 4.5),
            egui::pos2(c.x - 2.7, c.y + 5.2),
        ],
        stroke,
    );
    painter.line_segment(
        [
            egui::pos2(c.x - 2.7, c.y + 5.2),
            egui::pos2(c.x - 2.0, c.y + 3.7),
        ],
        stroke,
    );
    // Cap: a short crossbar near the eraser end.
    painter.line_segment(
        [
            egui::pos2(c.x + 1.5, c.y - 4.5),
            egui::pos2(c.x + 4.5, c.y - 1.5),
        ],
        stroke,
    );
    resp
}

/// Small "+" icon — a universal creation affordance (New goal, Attach…).
/// Painter-drawn for the same font-coverage reason as every other icon in
/// this file.
pub fn painted_plus_button(ui: &mut egui::Ui) -> egui::Response {
    let (rect, resp) =
        ui.allocate_exact_size(egui::vec2(ICON_SIZE, ICON_SIZE), egui::Sense::click());
    let color = if resp.hovered() { sky() } else { weak_text() };
    let stroke = egui::Stroke::new(1.4, color);
    let painter = ui.painter();
    let c = rect.center();
    painter.line_segment(
        [egui::pos2(c.x - 5.0, c.y), egui::pos2(c.x + 5.0, c.y)],
        stroke,
    );
    painter.line_segment(
        [egui::pos2(c.x, c.y - 5.0), egui::pos2(c.x, c.y + 5.0)],
        stroke,
    );
    resp
}

/// Small checkmark icon — the "confirm/submit" affordance (e.g. a manual
/// goal's weekly check-in, Goals place TASK-101).
pub fn painted_check_button(ui: &mut egui::Ui) -> egui::Response {
    let (rect, resp) =
        ui.allocate_exact_size(egui::vec2(ICON_SIZE, ICON_SIZE), egui::Sense::click());
    let color = if resp.hovered() { green() } else { weak_text() };
    let stroke = egui::Stroke::new(1.6, color);
    let painter = ui.painter();
    let c = rect.center();
    painter.line_segment(
        [egui::pos2(c.x - 4.0, c.y), egui::pos2(c.x - 1.0, c.y + 3.5)],
        stroke,
    );
    painter.line_segment(
        [
            egui::pos2(c.x - 1.0, c.y + 3.5),
            egui::pos2(c.x + 4.5, c.y - 4.0),
        ],
        stroke,
    );
    resp
}

/// Small circular-arrow icon — the "retry" affordance (Digest's attention
/// feed, TASK-99: re-flagging a failed dispatch). Painter-drawn for the same
/// font-coverage reason as every other icon in this file: a literal `↻`
/// renders as a tofu square on a stock install, the same failure this
/// module's header doc already documents for `●▸▾↑↓✕•○`.
///
/// Approximated as a ~300° arc (a polyline sampled around a circle, leaving
/// a gap for the arrowhead) plus a small triangular arrowhead at the open
/// end — `Painter` has no arc primitive, so this is built the same way
/// [`favorite_star_button`] builds its star: line segments and one filled
/// `convex_polygon`.
pub fn painted_retry_button(ui: &mut egui::Ui) -> egui::Response {
    let (rect, resp) =
        ui.allocate_exact_size(egui::vec2(ICON_SIZE, ICON_SIZE), egui::Sense::click());
    let color = if resp.hovered() { sky() } else { weak_text() };
    let stroke = egui::Stroke::new(1.4, color);
    let painter = ui.painter();
    let c = rect.center();
    let radius = 4.5;
    const ARC_STEPS: usize = 16;
    let start_deg: f32 = -40.0;
    let sweep_deg: f32 = 300.0;
    let points: Vec<egui::Pos2> = (0..=ARC_STEPS)
        .map(|i| {
            let t = (start_deg + sweep_deg * (i as f32 / ARC_STEPS as f32)).to_radians();
            egui::pos2(c.x + t.cos() * radius, c.y + t.sin() * radius)
        })
        .collect();
    painter.add(egui::Shape::line(points.clone(), stroke));
    if let (Some(&tip), Some(&prev)) = (points.last(), points.get(points.len().saturating_sub(2))) {
        let dir = (tip - prev).normalized();
        let normal = egui::vec2(-dir.y, dir.x);
        let head = vec![
            tip + dir * 1.5,
            tip - dir * 2.5 + normal * 2.2,
            tip - dir * 2.5 - normal * 2.2,
        ];
        painter.add(egui::Shape::convex_polygon(head, color, egui::Stroke::NONE));
    }
    resp
}

/// Small document icon — the "open log" affordance (Digest's attention
/// feed, TASK-99: opening a dispatch run's captured log file). Painter-drawn
/// for the same font-coverage reason as every other icon in this file; a
/// literal `≡` renders as a tofu square on a stock install.
pub fn painted_log_button(ui: &mut egui::Ui) -> egui::Response {
    let (rect, resp) =
        ui.allocate_exact_size(egui::vec2(ICON_SIZE, ICON_SIZE), egui::Sense::click());
    let color = if resp.hovered() { sky() } else { weak_text() };
    let stroke = egui::Stroke::new(1.2, color);
    let painter = ui.painter();
    let c = rect.center();
    painter.rect_stroke(
        egui::Rect::from_center_size(c, egui::vec2(9.0, 11.0)),
        1.0,
        stroke,
        egui::StrokeKind::Outside,
    );
    for dy in [-3.0, 0.0, 3.0] {
        painter.line_segment(
            [
                egui::pos2(c.x - 2.7, c.y + dy),
                egui::pos2(c.x + 2.7, c.y + dy),
            ],
            egui::Stroke::new(1.0, color),
        );
    }
    resp
}

/// Small X-cross icon — the "kill" affordance (Digest's attention feed,
/// TASK-99: stopping a stalled dispatch run or a port-squatting process).
/// Painter-drawn for the same font-coverage reason as every other icon in
/// this file — `✕` is explicitly one of the glyphs this module's header doc
/// names as rendering tofu on a stock install. Hover color mirrors
/// [`painted_trash_button`]'s destructive-on-hover treatment rather than a
/// filled `danger_button`, so every destructive icon action in the app reads
/// the same way.
pub fn painted_kill_button(ui: &mut egui::Ui) -> egui::Response {
    let (rect, resp) =
        ui.allocate_exact_size(egui::vec2(ICON_SIZE, ICON_SIZE), egui::Sense::click());
    let color = if resp.hovered() {
        danger()
    } else {
        weak_text()
    };
    let stroke = egui::Stroke::new(1.6, color);
    let painter = ui.painter();
    let c = rect.center();
    let r = 4.0;
    painter.line_segment(
        [egui::pos2(c.x - r, c.y - r), egui::pos2(c.x + r, c.y + r)],
        stroke,
    );
    painter.line_segment(
        [egui::pos2(c.x - r, c.y + r), egui::pos2(c.x + r, c.y - r)],
        stroke,
    );
    resp
}

/// Sets an icon-only button's AccessKit name to `label` and its hover
/// tooltip to the same text — the IA V2 trajectory entry's implementation
/// obligation that universal-action icon buttons derive their accessible
/// names from the same command-verb authority that names them everywhere
/// (AccessKit label equals the verb name). Every painted icon helper in
/// this file returns a bare `Response` with no widget text for AccessKit to
/// derive a name from, and `.on_hover_text` alone does **not** set one (see
/// `ui::backlog::board`'s note on `accesskit_consumer::Node::labelled_by`);
/// pair every icon button with this instead of `.on_hover_text` directly.
pub fn icon_button_label(resp: egui::Response, label: &str) -> egui::Response {
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, label));
    resp.on_hover_text(label)
}

/// The star affordance for favoriting an object (TASK-96) — a task, goal,
/// project, or saved view. `favorited` picks the filled (favorited) vs
/// outlined (not) star; painted for the same font-coverage reason as
/// [`Glyph`]. One shared definition so a task's, a goal's, a project's, and
/// a saved view's star always look identical — "same-purpose controls use
/// the same component" (`VISUAL_QA_CHECKLIST.md`).
///
/// Drawn as two overlapping equilateral triangles (a six-point "sparkle"
/// star) rather than a single five-point outline: `egui::Shape::
/// convex_polygon` — this file's one fill primitive for a closed shape — is
/// documented as convex-only, and a true five-point star silhouette is
/// concave. Two triangles are each individually convex, so the fill is
/// always tessellated correctly, and the composite still reads unambiguously
/// as "star" at icon size.
pub fn favorite_star_button(ui: &mut egui::Ui, favorited: bool) -> egui::Response {
    let (rect, resp) =
        ui.allocate_exact_size(egui::vec2(ICON_SIZE, ICON_SIZE), egui::Sense::click());
    let color = if favorited {
        amber()
    } else if resp.hovered() {
        weak_text()
    } else {
        muted_text()
    };
    let c = rect.center();
    let r = 5.2;
    let up = triangle_points(c, r, 0.0);
    let down = triangle_points(c, r, std::f32::consts::PI);
    if favorited {
        let painter = ui.painter();
        painter.add(egui::Shape::convex_polygon(
            up.to_vec(),
            color,
            egui::Stroke::NONE,
        ));
        painter.add(egui::Shape::convex_polygon(
            down.to_vec(),
            color,
            egui::Stroke::NONE,
        ));
    } else {
        let stroke = egui::Stroke::new(1.1, color);
        let painter = ui.painter();
        for tri in [up, down] {
            for i in 0..3 {
                painter.line_segment([tri[i], tri[(i + 1) % 3]], stroke);
            }
        }
    }
    resp
}

/// Three points of an equilateral triangle centered on `c` with circumradius
/// `r`, rotated by `offset_rad` — `0.0` points up, `PI` points down. Shared
/// by both triangles [`favorite_star_button`] overlays into a sparkle.
fn triangle_points(c: egui::Pos2, r: f32, offset_rad: f32) -> [egui::Pos2; 3] {
    std::array::from_fn(|i| {
        let angle =
            -std::f32::consts::FRAC_PI_2 + offset_rad + (i as f32) * std::f32::consts::TAU / 3.0;
        let (sin, cos) = angle.sin_cos();
        egui::pos2(c.x + cos * r, c.y + sin * r)
    })
}

/// IA V2 sidebar glyphs (TASK-96) — the nav's five place icons plus the two
/// favorite-type icons not already covered by a place (Project, View; the
/// Task and Goal favorite glyphs reuse [`Glyph::Tasks`]/[`Glyph::Goals`],
/// matching the frozen mock, which reuses the exact same ▤/◎ for both).
///
/// Painted via [`Painter`][egui::Painter], **not** rendered as literal
/// Unicode characters (⌂ ▤ ⌁ ◎ ⚙ ▣ ⌕ in the mock). This module's header doc
/// already established why for this exact family of geometric/symbol code
/// points: `●▸▾↑↓✕•○` rendered as empty tofu squares on a stock install
/// before `caret_button`/`triangle_button`/`painted_dot`/
/// `painted_trash_button` replaced them, and nothing about *this* set of
/// glyphs is known to be covered by the embedded Barlow/JetBrains fonts or
/// their fallbacks — TASK-96 chose the proven-safe path over spending a test
/// run confirming font coverage that could silently regress on a future
/// font swap. `⚙` is the one glyph in the set already used verbatim
/// elsewhere in this app (the Settings button) and evidently renders, but it
/// is painted here too rather than left as text: the Visual QA convention
/// this repo holds itself to bans mixing icon treatments in one row (see
/// `VISUAL_QA_CHECKLIST.md`'s Components section, "Icons use a consistent
/// family, stroke/fill treatment, and optical size").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Glyph {
    /// ⌂ — Digest place.
    Digest,
    /// ▤ — Tasks place, and the Task-kind favorite glyph.
    Tasks,
    /// ⌁ — Command place.
    Command,
    /// ◎ — Goals place, and the Goal-kind favorite glyph.
    Goals,
    /// ⚙ — Ops place.
    Ops,
    /// ▣ — Project-kind favorite glyph.
    Project,
    /// ⌕ — View-kind (saved view) favorite glyph.
    View,
}

/// Paint one [`Glyph`] into an `ICON_SIZE` box and return the (hover-only)
/// response, matching every other painted-icon helper in this file. Callers
/// that need a click wrap the allocated rect themselves (the nav's place
/// rows sense clicks on the whole row, not the icon alone) — see
/// `ui::nav`.
pub fn painted_glyph(ui: &mut egui::Ui, glyph: Glyph, color: Color32) -> egui::Response {
    let (rect, resp) =
        ui.allocate_exact_size(egui::vec2(ICON_SIZE, ICON_SIZE), egui::Sense::hover());
    let c = rect.center();
    let stroke = egui::Stroke::new(1.3, color);
    let painter = ui.painter();
    match glyph {
        Glyph::Digest => {
            // Roofline peak + a square body outline — the universal "home"
            // silhouette, simplified to two line segments and a rect.
            painter.line_segment(
                [egui::pos2(c.x - 5.0, c.y - 0.5), egui::pos2(c.x, c.y - 5.0)],
                stroke,
            );
            painter.line_segment(
                [egui::pos2(c.x, c.y - 5.0), egui::pos2(c.x + 5.0, c.y - 0.5)],
                stroke,
            );
            painter.rect_stroke(
                egui::Rect::from_center_size(egui::pos2(c.x, c.y + 2.5), egui::vec2(7.0, 5.0)),
                1.0,
                stroke,
                egui::StrokeKind::Outside,
            );
        }
        Glyph::Tasks => {
            // A bordered rect with three horizontal "row" strokes — the
            // primary work list's own icon, reused verbatim for a favorited
            // task.
            painter.rect_stroke(
                egui::Rect::from_center_size(c, egui::vec2(10.0, 10.0)),
                1.5,
                stroke,
                egui::StrokeKind::Outside,
            );
            for dy in [-2.5, 0.5, 3.5] {
                painter.line_segment(
                    [
                        egui::pos2(c.x - 3.2, c.y + dy),
                        egui::pos2(c.x + 3.2, c.y + dy),
                    ],
                    egui::Stroke::new(1.0, color),
                );
            }
        }
        Glyph::Command => {
            // A simplified lightning-bolt polyline — the fleet console's
            // "agents are actively doing things" mark.
            let pts = [
                egui::pos2(c.x + 1.5, c.y - 6.0),
                egui::pos2(c.x - 3.0, c.y + 0.5),
                egui::pos2(c.x + 0.3, c.y + 0.5),
                egui::pos2(c.x - 1.5, c.y + 6.0),
                egui::pos2(c.x + 3.5, c.y - 1.0),
                egui::pos2(c.x + 0.2, c.y - 1.0),
            ];
            painter.add(egui::Shape::convex_polygon(
                pts.to_vec(),
                color,
                egui::Stroke::NONE,
            ));
        }
        Glyph::Goals => {
            // Concentric rings — a target, reused for a favorited goal.
            painter.circle_stroke(c, 5.5, stroke);
            painter.circle_filled(c, 1.8, color);
        }
        Glyph::Ops => {
            // A hub circle with six short radial teeth — a simplified gear,
            // painted rather than left as the "⚙" text glyph so the whole
            // place row shares one icon treatment (see this enum's doc).
            painter.circle_stroke(c, 3.2, stroke);
            for i in 0..6 {
                let angle = (i as f32) * std::f32::consts::TAU / 6.0;
                let (sin, cos) = angle.sin_cos();
                let inner = egui::pos2(c.x + cos * 4.2, c.y + sin * 4.2);
                let outer = egui::pos2(c.x + cos * 6.2, c.y + sin * 6.2);
                painter.line_segment([inner, outer], stroke);
            }
        }
        Glyph::Project => {
            // A filled square with a visible border — "a grouping you can
            // hold in your hand", distinct from Tasks' open row-lined rect.
            let square = egui::Rect::from_center_size(c, egui::vec2(8.0, 8.0));
            painter.rect_filled(square, 1.0, scale_alpha(color, 0.35));
            painter.rect_stroke(square, 1.0, stroke, egui::StrokeKind::Outside);
        }
        Glyph::View => {
            // A magnifying glass — lens + handle.
            let lens_center = egui::pos2(c.x - 1.0, c.y - 1.0);
            painter.circle_stroke(lens_center, 3.6, stroke);
            let handle_dir = egui::vec2(0.72, 0.72);
            painter.line_segment(
                [
                    lens_center + handle_dir * 3.6,
                    lens_center + handle_dir * 6.6,
                ],
                stroke,
            );
        }
    }
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
    // egui 0.36 routes `TextEdit`'s placeholder through
    // `Visuals::weak_text_color()`, which by default is the body color gamma-
    // multiplied by `weak_text_alpha` — the same blend-toward-the-panel
    // problem this module's doc describes for `.weak()`, and it lands every
    // hint at ~3.7:1 on the light board (caught by `legibility_audit`, 42
    // runs across every lens). 0.36 also added this override, so the fix is
    // to point it at the muted role that is already AA-verified rather than
    // to restate an alpha.
    visuals.weak_text_color = Some(palette.muted_text);
    visuals.panel_fill = palette.panel_fill;
    visuals.window_fill = palette.panel_fill;
    visuals.faint_bg_color = palette.faint_bg;
    // Owner UX pass (2026-08-05): `extreme_bg_color` is egui's own name for
    // "TextEdit background" (see `TextEdit::background_color`'s doc) — this
    // codebase had repurposed that exact slot for *card* surfaces instead,
    // so every text input shared a fill with every flight-strip card,
    // undifferentiated. Pointing it at `faint_bg` (recessed — a field you
    // type *into*, the opposite metaphor from a raised card you read) fixes
    // that; cards now read `theme::card_bg()` explicitly instead of this
    // egui slot (board.rs, digest.rs, stats.rs).
    visuals.extreme_bg_color = palette.faint_bg;
    visuals.code_bg_color = palette.faint_bg;
    visuals.window_corner_radius = egui::CornerRadius::same(8);
    visuals.menu_corner_radius = egui::CornerRadius::same(7);
    visuals.window_stroke = surface_stroke();
    visuals.window_shadow = egui::epaint::Shadow {
        offset: [0, 8],
        blur: 20,
        spread: 0,
        color: Color32::from_black_alpha(64),
    };
    visuals.popup_shadow = egui::epaint::Shadow {
        offset: [0, 6],
        blur: 14,
        spread: 0,
        color: Color32::from_black_alpha(56),
    };
    // Distinct focus ring: at rest, an input's border reads as `weak_text`
    // at low opacity (bringing back the "pop" egui's own TextEdit code
    // comments admit stock visuals lack — see `interact()`'s call site in
    // egui's text_edit/builder.rs); focused/hovered, it switches to a
    // visible `sky()` ring, the same accent color links and the active
    // theme toggle already use, so "this field has focus" reads
    // unambiguously in both themes.
    visuals.widgets.inactive.bg_stroke =
        egui::Stroke::new(1.0, scale_alpha(palette.weak_text, 0.35));
    visuals.widgets.hovered.bg_stroke = egui::Stroke::new(1.5, palette.sky);
    visuals.widgets.active.bg_stroke = egui::Stroke::new(1.5, palette.sky);
    visuals.widgets.inactive.weak_bg_fill = palette.nav_bg;
    visuals.widgets.hovered.weak_bg_fill = palette.card_bg;
    visuals.widgets.active.weak_bg_fill = palette.card_bg;
    visuals.widgets.open.weak_bg_fill = palette.card_bg;
    visuals.widgets.inactive.fg_stroke.color = palette.weak_text;
    visuals.widgets.hovered.fg_stroke.color = palette.weak_text;
    visuals.widgets.active.fg_stroke.color = palette.weak_text;
    visuals.widgets.open.fg_stroke.color = palette.weak_text;
    for widget in [
        &mut visuals.widgets.noninteractive,
        &mut visuals.widgets.inactive,
        &mut visuals.widgets.hovered,
        &mut visuals.widgets.active,
        &mut visuals.widgets.open,
    ] {
        widget.corner_radius = egui::CornerRadius::same(5);
    }
    // Selectable labels use this fill as a literal paint color. Keep it
    // opaque (rather than an alpha-only accent) so both the rasterized UI
    // and the legibility audit see the same high-contrast surface.
    visuals.selection.bg_fill = palette.card_bg;
    visuals.selection.stroke = egui::Stroke::new(1.5, palette.weak_text);
    visuals.interact_cursor = Some(egui::CursorIcon::PointingHand);
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
    // Cursor blink is behaviour, not palette: rebuilding `Visuals` every
    // frame must not flip it back on after a host turned it off (the
    // headless test harness disables it so a focused text field settles
    // instead of requesting an immediate repaint at each blink boundary).
    visuals.text_cursor.blink = ctx.style_of(ctx.theme()).visuals.text_cursor.blink;
    ctx.set_visuals(visuals);

    // egui ships `Small` at 9pt — below the legibility floor. Raise it to the
    // floor so every `.small()` call site clears the size contract in one move,
    // while staying a step below `Body` (12.5pt) so the hierarchy survives.
    ctx.all_styles_mut(|style| {
        style.text_styles.insert(
            egui::TextStyle::Small,
            egui::FontId::proportional(legibility::MIN_FONT_POINTS),
        );
        style.spacing.item_spacing = egui::vec2(7.0, 5.0);
        style.spacing.button_padding = egui::vec2(8.0, 4.0);
        style.spacing.interact_size.y = 24.0;
        style.spacing.window_margin = egui::Margin::same(10);
        style.spacing.menu_margin = egui::Margin::same(8);
    });
}
