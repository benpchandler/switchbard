//! OKLCH lightness interpolation for the working-row pulse (TASK-241).
//!
//! APCA cares about sRGB luminance, but "does this look like it's breathing"
//! is a perceptual-lightness question: a channel-value scale that reads as an
//! 8% swing in gamma space can be a 2-3% swing in OKLab `L`, which is why the
//! old scale-based pulse on `darkroom` was reported as barely perceptible.
//! Converting to OKLCH, moving only `L`, and converting back keeps hue and
//! chroma exactly fixed (WCAG 2.3.1: "never hue") while making the lightness
//! swing the one deliberate, tunable knob.
//!
//! The declared `working.bg` is always the darker OKLCH endpoint (TASK-218:
//! the row must never dim below its own rest color); `DeclaredEndpoint` only
//! decides whether that declared color plays `glow` 0.0 or `glow` 1.0.
//!
//! Reference: Björn Ottosson, <https://bottosson.github.io/posts/oklab/>.

use ratatui::style::Color;

/// Trough-to-peak swing on dark canvases (berg, bloomberg, darkroom), an
/// absolute OKLCH lightness delta on the 0.0-1.0 `L` scale
/// (`docs/tui-formatting-legibility.md` section 4: "modulate lightness only
/// by 15-25%, never hue, never full on/off"). The declared `working.bg` is
/// the `glow` 0.0 trough (TASK-218: the floor the pulse never dims below);
/// the peak is this much lighter. 0.20, the top of the nominal band, is the
/// largest verified (not assumed) to still clear the Lc 75 working-row
/// floor at the peak, with `WORKING_TEXT_LIFT`, on all three dark presets;
/// darkroom is the tightest of the three, with a bit over 2 Lc of margin.
pub const WORK_LIGHTNESS_SWING_DARK: f64 = 0.20;

/// Trough-to-peak swing on the light canvas, the mirror of
/// `WORK_LIGHTNESS_SWING_DARK`: the declared `working.bg` is the `glow` 1.0
/// peak instead, and the trough is this much lighter.
///
/// This sits below the nominal 15-25% band, at 12%, for a reason proven
/// rather than assumed: `light`'s declared peak needs to sit at OKLab L >=
/// ~0.85 to have any chance at the Lc 75 working-row floor even with
/// maximum ink lift (below that, the best achievable contrast at any
/// hue/chroma tops out in the low 70s), which puts the lighter endpoint
/// within a hair of the sRGB gamut's white wall. TASK-237's `h7` highlight
/// slot (`crate::highlight::derive_fill`) is a pale, low-chroma fill
/// derived from `light`'s own declared body ink, so it and the pulse's
/// near-white endpoint bound the same ink from opposite directions: body
/// ink dark enough to clear the `h7` floor of Lc 75 is, at a 15% swing,
/// dark enough to push the near-white endpoint's contrast against it past
/// Lc 100 (the ceiling `tests/legibility.rs` gates). Exhaustive search
/// across ink lightness and hue at 15% found zero values clearing both;
/// 12% is the largest swing with real margin on every bound.
pub const WORK_LIGHTNESS_SWING_LIGHT: f64 = 0.12;

/// Which glow value (see `crate::app::App::work_glow`) the declared
/// `working.bg` renders at, and how far the other endpoint lies from it.
/// Dark and light canvases pick different variants of both (`Theme::
/// canvas_is_light`, the same place that already decides this), so this is
/// the one thing that decides which way the pulse reads: rising from a dim
/// declared color (dark canvases, whose declared color already sits close
/// to a bright ink) or dimming from a declared color that is itself the
/// closest-to-ink point (light canvases).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeclaredEndpoint {
    /// Dark canvases: the declared color is the `glow` 0.0 trough (the
    /// TASK-218 floor), and the pulse brightens toward `glow` 1.0 by
    /// `WORK_LIGHTNESS_SWING_DARK`.
    Trough,
    /// Light canvases: the declared color is the `glow` 1.0 peak (closest to
    /// a dark ink), and the pulse brightens toward `glow` 0.0 by
    /// `WORK_LIGHTNESS_SWING_LIGHT`.
    Peak,
}

/// Interpolate `declared`'s OKLCH lightness between itself and an endpoint
/// `swing` lighter, holding chroma and hue fixed, placing `declared` at
/// `glow` 0.0 or 1.0 per `endpoint`. `declared` must be a declared sRGB
/// color; a terminal-owned color (no preset background) passes through
/// unchanged since there is nothing to interpolate.
pub fn pulse_lightness(
    declared: Color,
    endpoint: DeclaredEndpoint,
    swing: f64,
    glow: f64,
) -> Color {
    let Color::Rgb(r, g, b) = declared else {
        return declared;
    };
    let (declared_l, chroma, hue) = rgb_to_oklch(r, g, b);
    debug_assert!((0.0..=1.0).contains(&declared_l), "OKLab L is normalized");
    let lighter_l = (declared_l + swing).min(1.0);
    let (trough_l, peak_l) = match endpoint {
        DeclaredEndpoint::Trough => (declared_l, lighter_l),
        DeclaredEndpoint::Peak => (lighter_l, declared_l),
    };
    let l = trough_l + (peak_l - trough_l) * glow.clamp(0.0, 1.0);
    oklch_to_rgb(l, chroma, hue)
}

/// sRGB (0-255 per channel) to OKLCH: `(L, C, H)`, `H` in radians.
fn rgb_to_oklch(r: u8, g: u8, b: u8) -> (f64, f64, f64) {
    let (r, g, b) = (srgb_to_linear(r), srgb_to_linear(g), srgb_to_linear(b));
    let l_lms = 0.412_221_470_8 * r + 0.536_332_536_3 * g + 0.051_445_992_9 * b;
    let m_lms = 0.211_903_498_2 * r + 0.680_699_545_1 * g + 0.107_396_956_6 * b;
    let s_lms = 0.088_302_461_9 * r + 0.281_718_837_6 * g + 0.629_978_700_5 * b;
    let (l_, m_, s_) = (l_lms.cbrt(), m_lms.cbrt(), s_lms.cbrt());
    let lightness = 0.210_454_255_3 * l_ + 0.793_617_785_0 * m_ - 0.004_072_046_8 * s_;
    let a = 1.977_998_495_1 * l_ - 2.428_592_205_0 * m_ + 0.450_593_709_9 * s_;
    let b_axis = 0.025_904_037_1 * l_ + 0.782_771_766_2 * m_ - 0.808_675_766_0 * s_;
    debug_assert!(
        lightness.is_finite(),
        "sRGB input always yields finite OKLab L"
    );
    (lightness, a.hypot(b_axis), b_axis.atan2(a))
}

/// The number of bisection steps `oklch_to_rgb` spends narrowing an
/// out-of-gamut chroma onto the display gamut boundary: a fixed, small
/// bound (Power-of-10 rule 2), and 24 steps is already sub-1e-7 precision
/// on a 0.0-1.0 chroma scale.
const GAMUT_SEARCH_STEPS: u32 = 24;

/// `(L, C, H)` to raw linear-light RGB, unclamped: the gamut test needs the
/// pre-clamp values to know whether a given chroma actually fits.
fn oklch_to_linear_rgb(lightness: f64, chroma: f64, hue: f64) -> (f64, f64, f64) {
    let (a, b_axis) = (chroma * hue.cos(), chroma * hue.sin());
    let l_ = lightness + 0.396_337_777_4 * a + 0.215_803_757_3 * b_axis;
    let m_ = lightness - 0.105_561_345_8 * a - 0.063_854_172_8 * b_axis;
    let s_ = lightness - 0.089_484_177_5 * a - 1.291_485_548_0 * b_axis;
    let (l_lms, m_lms, s_lms) = (l_.powi(3), m_.powi(3), s_.powi(3));
    let r = 4.076_741_662_1 * l_lms - 3.307_711_591_3 * m_lms + 0.230_969_929_2 * s_lms;
    let g = -1.268_438_004_6 * l_lms + 2.609_757_401_1 * m_lms - 0.341_319_396_5 * s_lms;
    let b = -0.004_196_086_3 * l_lms - 0.703_418_614_7 * m_lms + 1.707_614_701_0 * s_lms;
    (r, g, b)
}

fn in_gamut((r, g, b): (f64, f64, f64)) -> bool {
    const EPSILON: f64 = 1e-7;
    let bounds = -EPSILON..=1.0 + EPSILON;
    bounds.contains(&r) && bounds.contains(&g) && bounds.contains(&b)
}

/// OKLCH `(L, C, H radians)` back to sRGB. Near black or white, the
/// requested chroma can fall outside what any hue can render at that
/// lightness (a real property of the sRGB gamut, not a bug: peak chroma
/// shrinks to zero at `L` 0 and 1). Rather than clamp each channel
/// independently, which would drag `L` and `H` off their requested values
/// and show up as an off-white or muddy tint at the pulse's extremes, this
/// holds `L` and `H` exactly and bisects `C` down to the gamut boundary.
fn oklch_to_rgb(lightness: f64, chroma: f64, hue: f64) -> Color {
    let full = oklch_to_linear_rgb(lightness, chroma, hue);
    let linear = if in_gamut(full) {
        full
    } else {
        let (mut lo, mut hi) = (0.0, chroma);
        for _ in 0..GAMUT_SEARCH_STEPS {
            let mid = (lo + hi) / 2.0;
            if in_gamut(oklch_to_linear_rgb(lightness, mid, hue)) {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        oklch_to_linear_rgb(lightness, lo, hue)
    };
    debug_assert!(
        linear.0.is_finite() && linear.1.is_finite() && linear.2.is_finite(),
        "OKLab inputs stay finite"
    );
    let (r, g, b) = linear;
    Color::Rgb(linear_to_srgb(r), linear_to_srgb(g), linear_to_srgb(b))
}

fn srgb_to_linear(channel: u8) -> f64 {
    let c = f64::from(channel) / 255.0;
    if c <= 0.040_45 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

/// Inverse of `srgb_to_linear`, clamped to a valid channel: OKLCH round
/// trips can overshoot the gamut slightly at the lightness extremes this
/// module targets (near black, near white).
fn linear_to_srgb(channel: f64) -> u8 {
    let clamped = channel.clamp(0.0, 1.0);
    let encoded = if clamped <= 0.003_130_8 {
        12.92 * clamped
    } else {
        1.055 * clamped.powf(1.0 / 2.4) - 0.055
    };
    (encoded.clamp(0.0, 1.0) * 255.0).round() as u8
}
