//! Shared column-width measurement pass.
//!
//! Why: every table in the app needs columns sized to fit content without
//! clipping, and stacked sub-tables in the same view need *matching* widths
//! so vertical scanning lines up. Each view walks every visible row once,
//! asks egui to measure each cell text, and passes the resulting per-column
//! max as `Column::initial(width)` to every sub-table.
//!
//! Measurement uses `Fonts::layout_no_wrap` — the same engine egui itself
//! uses to lay out labels, so widths match what the table will actually
//! render. Memoized, so repeated calls are cheap. No glyph constants, no
//! fudge factors.

use eframe::egui::{self, FontId, TextStyle};

/// Padding added to every column so text doesn't sit flush against the cell
/// border / next column's content.
pub const COL_PADDING: f32 = 16.0;

/// Variant of the body font a cell renders in. Drives which FontId we use to
/// measure.
#[derive(Debug, Clone, Copy)]
pub enum CellFont {
    /// Default body font (Ubuntu-Light in egui 0.29).
    Proportional,
    /// Monospace body font (Hack in egui 0.29).
    Monospace,
}

impl CellFont {
    fn font_id(self, style: &egui::Style) -> FontId {
        match self {
            Self::Proportional => TextStyle::Body.resolve(style),
            Self::Monospace => TextStyle::Monospace.resolve(style),
        }
    }
}

/// Measure the rendered width of `text` in the given cell font, using egui's
/// own layout engine. Result has no padding.
pub fn measure(ctx: &egui::Context, text: &str, font: CellFont) -> f32 {
    let font_id = font.font_id(&ctx.style());
    ctx.fonts(|f| {
        f.layout_no_wrap(text.to_owned(), font_id, egui::Color32::WHITE)
            .rect
            .width()
    })
}

/// Width of a column whose content is `cells` in the given cell font, with a
/// floor of `min_px`. Adds `COL_PADDING` for the gutter.
pub fn column_width<'a, I>(ctx: &egui::Context, cells: I, font: CellFont, min_px: f32) -> f32
where
    I: IntoIterator<Item = &'a str>,
{
    let widest = cells
        .into_iter()
        .map(|s| measure(ctx, s, font))
        .fold(0.0_f32, f32::max);
    (widest + COL_PADDING).max(min_px)
}
