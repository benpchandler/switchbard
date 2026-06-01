//! The legibility contract — the single source of truth for "can a user
//! actually read this text?".
//!
//! "Too small" and "too light gray" are not matters of taste; they are two
//! measurable properties with published floors. This module names those floors
//! and the math to check them, so a UI review can *assert* legibility instead
//! of eyeballing it. The `legibility_audit` integration test walks every
//! painted text run in the real views and fails on anything that trips a floor.
//!
//! Two axes, two anchors:
//! - **Size** — [`MIN_FONT_POINTS`], anchored to Apple's Human Interface
//!   Guidelines, which treat 11pt as the smallest comfortably legible size on
//!   macOS (this is a macOS-only app). egui's built-in `TextStyle::Small` is
//!   9.0pt — below the floor by construction, so any *content* rendered
//!   `.small()` trips the contract.
//! - **Contrast** — WCAG 2.1 AA: [`MIN_CONTRAST_NORMAL`] for normal text and
//!   [`MIN_CONTRAST_LARGE`] for large text (≥ [`LARGE_TEXT_POINTS`]). This is
//!   the same standard `theme.rs` already cites in prose for its chromatic
//!   constants; the audit promotes that prose into an executable check.
//!
//! Keeping the numbers and the math here (rather than inline in the test) means
//! a future palette or type-scale change is a one-file diff, and runtime code
//! could consume the same floors later (e.g. a debug overlay) without
//! re-deriving them.

use eframe::egui::Color32;

/// Smallest font size, in points, allowed for text the user is meant to read
/// (paths, previews, body, metadata). Apple HIG legibility floor for macOS.
pub const MIN_FONT_POINTS: f32 = 11.0;

/// WCAG 2.1 AA minimum contrast ratio for normal-size text.
pub const MIN_CONTRAST_NORMAL: f64 = 4.5;

/// WCAG 2.1 AA minimum contrast ratio for large text.
pub const MIN_CONTRAST_LARGE: f64 = 3.0;

/// WCAG's "large text" threshold, in points (regular weight). At or above this
/// size the relaxed [`MIN_CONTRAST_LARGE`] applies. (WCAG also relaxes for bold
/// ≥ 14pt; we model only the size axis, which is the conservative choice.)
pub const LARGE_TEXT_POINTS: f32 = 18.0;

/// Relative luminance of an (assumed-opaque) sRGB color, per WCAG 2.1.
pub fn relative_luminance(c: Color32) -> f64 {
    fn linearize(srgb: u8) -> f64 {
        let s = srgb as f64 / 255.0;
        if s <= 0.040_45 {
            s / 12.92
        } else {
            ((s + 0.055) / 1.055).powf(2.4)
        }
    }
    0.2126 * linearize(c.r()) + 0.7152 * linearize(c.g()) + 0.0722 * linearize(c.b())
}

/// WCAG contrast ratio between two opaque colors. Order-independent; the result
/// lies in `[1.0, 21.0]`. Composite any translucent foreground over its
/// background with [`composite_over`] *before* calling this — a contrast ratio
/// is only meaningful between the two colors a viewer actually perceives.
pub fn contrast_ratio(a: Color32, b: Color32) -> f64 {
    let (la, lb) = (relative_luminance(a), relative_luminance(b));
    let (hi, lo) = if la >= lb { (la, lb) } else { (lb, la) };
    (hi + 0.05) / (lo + 0.05)
}

/// Flatten a (premultiplied-alpha) foreground over an opaque background into the
/// single opaque color a viewer perceives. `Color32` stores premultiplied
/// alpha, so the over-operator is simply `fg + bg·(1 − αfg)`.
pub fn composite_over(fg: Color32, bg: Color32) -> Color32 {
    let inv = 1.0 - fg.a() as f32 / 255.0;
    let mix = |f: u8, b: u8| (f as f32 + b as f32 * inv).round().clamp(0.0, 255.0) as u8;
    Color32::from_rgb(
        mix(fg.r(), bg.r()),
        mix(fg.g(), bg.g()),
        mix(fg.b(), bg.b()),
    )
}

/// The minimum contrast ratio a run of the given point size must clear.
pub fn min_contrast_for(points: f32) -> f64 {
    if points >= LARGE_TEXT_POINTS {
        MIN_CONTRAST_LARGE
    } else {
        MIN_CONTRAST_NORMAL
    }
}
