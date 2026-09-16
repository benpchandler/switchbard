//! Highlight slots: the fills a theme offers paint rules by name (`h1`, `h2`,
//! ...). A preset declares its own in `theme.highlights`; where it omits one,
//! `derive_fill` builds it from the matching palette color so the slot stays
//! related to both that categorical hue and the canvas it sits on.

use ratatui::style::Color;

/// How many slots a theme can name. Nine keeps every slot two characters wide
/// and bounds the picker.
pub const MAX_SLOTS: usize = 9;

/// Slots every theme offers, declared or derived, so the picker always has
/// swatches to show.
pub const MINIMUM_SLOTS: usize = 3;

/// How far a derived fill sits from a dark canvas, in OKLCH lightness. Far
/// enough to read as a fill, near enough that body ink still carries.
const STEP_ON_DARK: f64 = 0.10;

/// The same step on a light canvas. Smaller, because lightness contrast falls
/// away faster at the bright end.
const STEP_ON_LIGHT: f64 = 0.055;

/// Above this OKLCH lightness the canvas counts as light and fills go darker.
const LIGHT_CANVAS: f64 = 0.5;

/// A derived fill is a tint, never a full-strength hue: chroma is capped here.
const MAX_FILL_CHROMA: f64 = 0.05;

/// `h3` -> `Some(3)`, one-based. Anything else names no slot.
#[must_use]
pub fn slot_index(token: &str) -> Option<usize> {
    let digits = token.strip_prefix('h')?;
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let index = digits.parse::<usize>().ok()?;
    (1..=MAX_SLOTS).contains(&index).then_some(index)
}

/// How slot `index` is written in a rule and in `theme.highlights`.
#[must_use]
pub fn slot_token(index: usize) -> String {
    debug_assert!((1..=MAX_SLOTS).contains(&index), "slots are one-based");
    format!("h{index}")
}

/// The fill for a slot the preset does not declare: the palette color's hue and
/// chroma, moved to a fixed lightness step from the canvas. Keeping the step
/// relative to the background is what makes the fill readable without the
/// preset having measured it. A terminal-owned canvas or seed has no sRGB
/// lightness to step from and derives nothing.
#[must_use]
pub fn derive_fill(seed: Color, canvas: Color) -> Option<Color> {
    let (canvas_lightness, ..) = to_oklab(canvas)?;
    let (_, a, b) = to_oklab(seed)?;
    let chroma = a.hypot(b);
    let lightness = if canvas_lightness < LIGHT_CANVAS {
        canvas_lightness + STEP_ON_DARK
    } else {
        canvas_lightness - STEP_ON_LIGHT
    };
    debug_assert!(
        (0.0..=1.0).contains(&lightness),
        "a derived fill stays inside the lightness range"
    );
    let scale = if chroma > MAX_FILL_CHROMA {
        MAX_FILL_CHROMA / chroma
    } else {
        1.0
    };
    Some(from_oklab(lightness, a * scale, b * scale))
}

/// sRGB -> OKLab (Björn Ottosson's matrices).
fn to_oklab(color: Color) -> Option<(f64, f64, f64)> {
    let Color::Rgb(red, green, blue) = color else {
        return None;
    };
    let [r, g, b] = [red, green, blue].map(to_linear);
    let long = (0.412_221_470_8 * r + 0.536_332_536_3 * g + 0.051_445_992_9 * b).cbrt();
    let medium = (0.211_903_498_2 * r + 0.680_699_545_1 * g + 0.107_396_956_6 * b).cbrt();
    let short = (0.088_302_461_9 * r + 0.281_718_837_6 * g + 0.629_978_700_5 * b).cbrt();
    Some((
        0.210_454_255_3 * long + 0.793_617_785_0 * medium - 0.004_072_046_8 * short,
        1.977_998_495_1 * long - 2.428_592_205_0 * medium + 0.450_593_709_9 * short,
        0.025_904_037_1 * long + 0.782_771_766_2 * medium - 0.808_675_766_0 * short,
    ))
}

/// OKLab -> sRGB, clamped into gamut channel by channel.
fn from_oklab(lightness: f64, a: f64, b: f64) -> Color {
    let long = (lightness + 0.396_337_777_4 * a + 0.215_803_757_3 * b).powi(3);
    let medium = (lightness - 0.105_561_345_8 * a - 0.063_854_172_8 * b).powi(3);
    let short = (lightness - 0.089_484_177_5 * a - 1.291_485_548_0 * b).powi(3);
    Color::Rgb(
        to_srgb(4.076_741_662_1 * long - 3.307_711_591_3 * medium + 0.230_969_929_2 * short),
        to_srgb(-1.268_438_004_6 * long + 2.609_757_401_1 * medium - 0.341_319_396_5 * short),
        to_srgb(-0.004_196_086_3 * long - 0.703_418_614_7 * medium + 1.707_614_701_0 * short),
    )
}

fn to_linear(channel: u8) -> f64 {
    let value = f64::from(channel) / 255.0;
    if value <= 0.040_45 {
        value / 12.92
    } else {
        ((value + 0.055) / 1.055).powf(2.4)
    }
}

fn to_srgb(channel: f64) -> u8 {
    let value = channel.clamp(0.0, 1.0);
    let encoded = if value <= 0.003_130_8 {
        value * 12.92
    } else {
        1.055 * value.powf(1.0 / 2.4) - 0.055
    };
    (encoded.clamp(0.0, 1.0) * 255.0).round() as u8
}
