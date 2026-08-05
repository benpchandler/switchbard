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
//! no selection, so the rail needs no separate empty-state handling of its
//! own — it just renders quieter (a single centered label) instead of a
//! full editor when nothing's selected.

use super::{Pending, Snapshot};
use crate::app::HiveApp;
use eframe::egui;

const MIN_WIDTH: f32 = 320.0;
const DEFAULT_WIDTH: f32 = 420.0;
const MAX_WIDTH: f32 = 720.0;

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
    egui::SidePanel::right("backlog_detail_rail")
        .resizable(true)
        .default_width(DEFAULT_WIDTH)
        .width_range(MIN_WIDTH..=MAX_WIDTH)
        .show(ctx, |ui| {
            super::detail::render_task_detail(app, ui, snap, pending);
        });
}
