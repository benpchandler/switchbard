//! TASK-41: the per-row bulk-select checkbox and the "Remove N worktrees"
//! confirmation modal. Split from `mod.rs` for the same reason `staleness.rs`
//! is — see that module's header doc.

use crate::app::HiveApp;
use crate::runtime::BulkRemoveCandidate;
use crate::ui::theme;
use eframe::egui;
use std::collections::BTreeSet;
use std::path::PathBuf;
use switchbard_core::WorktreeRef;

/// The bulk-select checkbox rendered in each non-primary worktree row's
/// trailing cluster. Primary worktrees never render one — `is_primary` is
/// the same cheap `w.path == repo.path` check `mod.rs` already computes for
/// its row, not the filesystem-canonicalizing `is_primary_worktree` (that
/// stronger guarantee is reserved for `HiveApp::open_bulk_remove_worktree_confirm`,
/// which drops any surviving primary from the selection defensively).
pub(super) fn render_select_checkbox(
    ui: &mut egui::Ui,
    w: &WorktreeRef,
    is_primary: bool,
    selected: &mut BTreeSet<PathBuf>,
) {
    if is_primary {
        return;
    }
    let mut checked = selected.contains(&w.path);
    if ui
        .checkbox(&mut checked, "")
        .on_hover_text("Select for bulk remove")
        .changed()
    {
        if checked {
            selected.insert(w.path.clone());
        } else {
            selected.remove(&w.path);
        }
    }
}

/// The bulk-remove confirmation modal. Reads `app.confirm_bulk_remove_worktrees`
/// once per frame, same shape as `render_remove_worktree_modal` for the
/// single-row dialog.
pub(super) fn render_modal(app: &mut HiveApp, ctx: &egui::Context) {
    let Some(state) = app.confirm_bulk_remove_worktrees.lock().unwrap().clone() else {
        return;
    };

    let mut do_confirm = false;
    let mut do_cancel = false;
    let mut delete_branches = state.delete_branches;

    egui::Window::new("Remove worktrees")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            ui.set_max_width(560.0);
            ui.label(
                egui::RichText::new(format!(
                    "Remove {} clean, merged worktree{}?",
                    state.removable.len(),
                    if state.removable.len() == 1 { "" } else { "s" }
                ))
                .strong(),
            );

            if state.removable.is_empty() {
                ui.add_space(4.0);
                ui.colored_label(
                    theme::amber(),
                    "Nothing here is safe to remove — every selected worktree landed in \
                     the needs-review list below.",
                );
            } else {
                egui::ScrollArea::vertical()
                    .max_height(160.0)
                    .id_salt("bulk_remove_removable")
                    .show(ui, |ui| {
                        for c in &state.removable {
                            render_candidate_line(ui, c, theme::green());
                        }
                    });
                ui.add_space(6.0);
                ui.checkbox(
                    &mut delete_branches,
                    "Also delete each branch (plain `git branch -d`, only ever run on \
                     already-merged branches — never force-deleted)",
                );
            }

            if !state.needs_review.is_empty() {
                ui.add_space(8.0);
                ui.label(
                    egui::RichText::new(format!(
                        "⚠ {} worktree{} left for review — never touched by this action:",
                        state.needs_review.len(),
                        if state.needs_review.len() == 1 {
                            ""
                        } else {
                            "s"
                        }
                    ))
                    .color(theme::amber()),
                );
                egui::ScrollArea::vertical()
                    .max_height(160.0)
                    .id_salt("bulk_remove_needs_review")
                    .show(ui, |ui| {
                        for c in &state.needs_review {
                            render_candidate_line(ui, c, theme::amber());
                        }
                    });
            }

            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button("Cancel").clicked() {
                    do_cancel = true;
                }
                let confirm_label = format!("Remove {}", state.removable.len());
                if ui
                    .add_enabled(
                        !state.removable.is_empty(),
                        theme::danger_button(&confirm_label),
                    )
                    .clicked()
                {
                    do_confirm = true;
                }
            });
        });

    if delete_branches != state.delete_branches {
        if let Some(s) = app.confirm_bulk_remove_worktrees.lock().unwrap().as_mut() {
            s.delete_branches = delete_branches;
        }
    }

    if do_confirm {
        app.execute_bulk_remove_worktrees(ctx);
    } else if do_cancel {
        app.cancel_bulk_remove_worktree_confirm();
    }
}

/// One candidate line. Carries name, branch, and (on hover) the full path.
///
/// The branch is not decoration here. A worktree's display name comes from its
/// directory, and some layouts name that directory after the *repo* rather
/// than the worktree — which rendered several identical lines in this dialog,
/// reading as an offer to delete the repo itself. `worktree_display_name` now
/// disambiguates at the source, but this is the one dialog that deletes
/// things, so it states the identity twice over rather than trusting a single
/// string to be unique.
fn render_candidate_line(ui: &mut egui::Ui, c: &BulkRemoveCandidate, dot_color: egui::Color32) {
    ui.horizontal(|ui| {
        theme::painted_dot(ui, dot_color);
        ui.add_space(2.0);
        ui.label(&c.display_name)
            .on_hover_text(c.worktree_path.display().to_string());
        match &c.branch {
            Some(branch) => {
                ui.label(
                    egui::RichText::new(branch)
                        .monospace()
                        .color(theme::muted_text()),
                );
            }
            None => {
                ui.label(
                    egui::RichText::new("detached")
                        .monospace()
                        .color(theme::weak_text()),
                );
            }
        }
        if let Some(reason) = &c.review_reason {
            ui.label(egui::RichText::new(format!("— {reason}")).color(theme::weak_text()));
        }
    });
}
