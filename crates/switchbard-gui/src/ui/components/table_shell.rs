//! Standardized TableBuilder defaults so every table in the app has the same
//! base styling (striped, resizable, single cell layout, vscroll off because
//! the outer ScrollArea owns scrolling). The caller adds columns + header +
//! body.

use eframe::egui;
use egui_extras::TableBuilder;

/// Construct a `TableBuilder` with Switchbard's shared defaults. The `id_salt`
/// scopes egui widget IDs so multiple stacked tables don't collide.
pub fn table_shell<'a>(ui: &'a mut egui::Ui, id_salt: impl std::hash::Hash) -> TableBuilder<'a> {
    TableBuilder::new(ui)
        .id_salt(id_salt)
        .vscroll(false)
        .striped(true)
        .resizable(true)
        .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
}
