//! Compute shared column widths across all per-repo tables in a tab.
//!
//! Why: each per-repo `TableBuilder` previously used `Column::auto()`, which
//! sizes itself based on *its own* content. Stacked tables ended up with
//! visibly different column widths, which made vertical scanning jarring.
//! The fix is to walk every row once at the start of the render, find the
//! widest content per column, and pass that as `Column::initial(width)` to
//! every per-repo table — they all line up.
//!
//! Pixel widths are estimated from char counts. egui's `Painter::layout`
//! could measure exactly but only inside a render callback; we want widths
//! before the first table opens. Per-glyph constants below were validated
//! visually against egui 0.29's default Ubuntu-Light and Hack fonts.

/// Approximate glyph width for proportional body text (Ubuntu-Light, body
/// font size). Slightly generous so wider glyphs (m, w) don't overflow.
pub const CHAR_W_PROPORTIONAL: f32 = 7.5;

/// Approximate glyph width for monospace body text (Hack, body font size).
/// Mono glyphs are uniform so this is closer to a true measurement.
pub const CHAR_W_MONOSPACE: f32 = 8.5;

/// Padding added to every column so the text doesn't sit flush against the
/// cell border / next column's content.
pub const COL_PADDING: f32 = 16.0;

/// Estimate the rendered width in pixels of `text` in either proportional or
/// monospace body font.
pub fn estimate_text_width(text: &str, monospace: bool) -> f32 {
    let glyph = if monospace {
        CHAR_W_MONOSPACE
    } else {
        CHAR_W_PROPORTIONAL
    };
    text.chars().count() as f32 * glyph
}

/// Width of a column whose content is `cells`, in `monospace` font, with a
/// floor of `min_px`. Add COL_PADDING for the gutter.
pub fn column_width<'a, I>(cells: I, monospace: bool, min_px: f32) -> f32
where
    I: IntoIterator<Item = &'a str>,
{
    let widest = cells
        .into_iter()
        .map(|s| estimate_text_width(s, monospace))
        .fold(0.0_f32, f32::max);
    (widest + COL_PADDING).max(min_px)
}
