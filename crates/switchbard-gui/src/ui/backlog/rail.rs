//! The persistent right-hand detail rail (owner UX pass, 2026-08-05):
//! whichever task is currently selected (`backlog_view.selected_task`)
//! shows its full detail here, regardless of which Backlog lens is active —
//! replacing the old "click a card → jump to the List lens" flow. Reuses
//! `detail::render_task_detail` unchanged: this is a different *placement*
//! for the same content, not a fork, so every existing detail-pane
//! mutation (Save, checklist toggles, notes, archive, dispatch) keeps
//! working exactly as it did embedded in List.
//!
//! `render_task_detail` already has its own "Select a task" empty state for
//! no selection. The rail adds only placement controls: its left edge is
//! drag-resizable, and its header can collapse it to a narrow edge toggle.

use super::{Pending, Snapshot};
use crate::app::HiveApp;
use crate::ui::theme;
use eframe::egui;

const MIN_WIDTH: f32 = 320.0;
const DEFAULT_WIDTH: f32 = 420.0;
const MAX_WIDTH: f32 = 720.0;
const COLLAPSED_WIDTH: f32 = 28.0;

/// Must render *before* the central panel (same ordering rule
/// `HiveApp::render_ui` documents for every side panel) so it claims its
/// docked space first. Shares the caller's `Snapshot`/`Pending` rather than
/// building its own — a Save click here queues into the exact same
/// `apply_pending` call the lens content's own mutations do, so there's one
/// CLI dispatch per frame, not two.
pub(super) fn render_detail_rail(
    app: &mut HiveApp,
    ctx: &egui::Context,
    snap: &Snapshot,
    pending: &mut Pending,
) {
    if app.backlog_view.detail_rail_collapsed {
        render_collapsed(app, ctx);
        return;
    }

    // Owner UX pass: `theme::rail_bg()` instead of the default panel fill
    // (`ctx.style()`'s own `side_top_panel` frame would otherwise match the
    // board's own `panel_fill`), so the rail reads as its own persistent
    // workspace tier rather than "more board."
    let frame = egui::Frame::side_top_panel(&ctx.style()).fill(theme::rail_bg());
    egui::SidePanel::right("backlog_detail_rail")
        .resizable(true)
        .default_width(DEFAULT_WIDTH)
        .width_range(MIN_WIDTH..=MAX_WIDTH)
        .frame(frame)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Task details").strong());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .small_button("▶")
                        .on_hover_text("Collapse the task-detail rail")
                        .clicked()
                    {
                        app.backlog_view.detail_rail_collapsed = true;
                    }
                });
            });
            ui.separator();
            super::detail::render_task_detail(app, ui, snap, pending);
        });
}

/// Keep a discoverable edge control when the rail is collapsed. A distinct
/// panel id preserves the expanded panel's user-resized width in egui's
/// persisted panel state instead of overwriting it with 28 points.
fn render_collapsed(app: &mut HiveApp, ctx: &egui::Context) {
    let frame = egui::Frame::side_top_panel(&ctx.style()).fill(theme::rail_bg());
    egui::SidePanel::right("backlog_detail_rail_collapsed")
        .resizable(false)
        .exact_width(COLLAPSED_WIDTH)
        .frame(frame)
        .show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(4.0);
                if ui
                    .small_button("◀")
                    .on_hover_text("Expand the task-detail rail")
                    .clicked()
                {
                    app.backlog_view.detail_rail_collapsed = false;
                }
            });
        });
}
