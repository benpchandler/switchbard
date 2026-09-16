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
//! Reference: Björn Ottosson, <https://bottosson.github.io/posts/oklab/>.

use ratatui::style::Color;

/// Trough-to-peak swing of the working pulse, an absolute OKLCH lightness
/// delta on the 0.0-1.0 `L` scale (`docs/tui-formatting-legibility.md`
/// section 4: "modulate lightness only by 15-25%, never hue, never full
/// on/off"). `PulseDirection` decides which side of the declared peak the
/// trough falls on; `pulse_lightness` clamps at the sRGB gamut edge, which
/// can shrink the realized swing below this nominal value (the light preset
/// clamps at white, landing at the low end of the 15-25% band).
pub const WORK_LIGHTNESS_SWING: f64 = 0.20;

/// Which way the trough recedes from the declared peak color. The peak is
/// always the color closest to the row's ink (built once from the theme's
/// `working.bg`), so receding further from the ink is always the direction
/// that gains contrast headroom.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PulseDirection {
    /// Dark canvases: the peak already sits close to a bright ink, so the
    /// trough deepens toward black.
    Darken,
    /// Light canvases: the peak sits close to a dark ink, so the trough
    /// lifts toward white.
    Lighten,
}

/// Interpolate `peak`'s OKLCH lightness between the declared value (`glow`
/// 1.0) and a trough `WORK_LIGHTNESS_SWING` away in `direction` (`glow`
/// 0.0), holding chroma and hue fixed. `peak` must be a declared sRGB color;
/// a terminal-owned color (no preset background) passes through unchanged
/// since there is nothing to interpolate.
pub fn pulse_lightness(peak: Color, direction: PulseDirection, glow: f64) -> Color {
    let Color::Rgb(r, g, b) = peak else {
        return peak;
    };
    let (peak_l, chroma, hue) = rgb_to_oklch(r, g, b);
    debug_assert!((0.0..=1.0).contains(&peak_l), "OKLab L is normalized");
    let trough_l = match direction {
        PulseDirection::Darken => (peak_l - WORK_LIGHTNESS_SWING).max(0.0),
        PulseDirection::Lighten => (peak_l + WORK_LIGHTNESS_SWING).min(1.0),
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
