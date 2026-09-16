//! Lightness contrast (Lc) over declared sRGB pairs, APCA-W3 0.0.98G-4g:
//! <https://apcaw3.myndex.com/docs/APCA-W3-LaTeX.html>. One implementation for
//! both readers: the theme refuses a fill no ink can be read on, and
//! `tests/legibility.rs` gates every rendered pair the presets can produce.
//! These are project design thresholds, not an accessibility certification.

use ratatui::style::Color;

/// The floor an ink must clear on a fill to count as readable at all. Secondary
/// ink (`quiet`) sits here; body ink is held far higher by the rendered gate.
pub const READABLE: f64 = 45.0;

/// Lc for one rendered pair. `None` when either side is terminal-owned: an ANSI
/// slot has no sRGB value here, so no numeric claim is possible.
#[must_use]
pub fn contrast(foreground: Color, background: Color) -> Option<f64> {
    let (Color::Rgb(..), Color::Rgb(..)) = (foreground, background) else {
        return None;
    };
    let text = screen_luminance(foreground)?;
    let canvas = screen_luminance(background)?;
    if (text - canvas).abs() < 0.0005 {
        return Some(0.0);
    }
    let raw = if canvas > text {
        (canvas.powf(0.56) - text.powf(0.57)) * 1.14
    } else {
        (canvas.powf(0.65) - text.powf(0.62)) * 1.14
    };
    let clamped = if raw.abs() < 0.1 {
        0.0
    } else {
        (raw - raw.signum() * 0.027) * 100.0
    };
    debug_assert!(clamped.is_finite(), "Lc is finite for declared sRGB");
    Some(clamped)
}

/// Whether ink clears the readable floor on a fill. Terminal-owned colors make
/// no claim either way, so they are not refused.
#[must_use]
pub fn reads_on(ink: Color, fill: Color) -> Option<bool> {
    contrast(ink, fill).map(|value| value.abs() >= READABLE)
}

/// APCA's screen luminance: a 2.4 exponent per channel, then the near-black
/// clamp that keeps very dark pairs from reading as higher contrast than they are.
fn screen_luminance(color: Color) -> Option<f64> {
    let Color::Rgb(r, g, b) = color else {
        return None;
    };
    let value: f64 = [(r, 0.2126729), (g, 0.7151522), (b, 0.0721750)]
        .into_iter()
        .map(|(channel, weight)| (f64::from(channel) / 255.0).powf(2.4) * weight)
        .sum();
    Some(if value < 0.022 {
        value + (0.022 - value).powf(1.414)
    } else {
        value
    })
}
