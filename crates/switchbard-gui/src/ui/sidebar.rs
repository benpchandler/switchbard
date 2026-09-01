//! The repo-removal confirmation modal — the one surviving piece of what
//! used to be the "Tracked repos" left-side panel (TASK-27, then the
//! 2026-08-05 owner UX pass that scoped it to the Servers/Workspace view).
//! TASK-100 retired that panel entirely: the merged Ops table is nav plus
//! the one-row-per-worktree table only, matching the frozen mock's §6
//! layout, and repo add/remove is reachable from the Settings window
//! (`ui::settings`) alone now — which already offered it, so nothing here
//! lost a capability, only a redundant second surface for it.
//! `Config.ui.sidebar_collapsed` and `HiveApp::expanded_repos` are left in
//! place, inert (harmless, unread) rather than migrated, matching this
//! repo's "unmatched old keys are dropped, not migrated by guess"
//! convention for retired UI state.

use crate::app::HiveApp;
use crate::ui::theme;
use eframe::egui;

/// Modal that pops over the whole window when the user clicks "Remove" next
/// to a repo (from either this panel or the Settings window's own repo
/// list). Confirm removes the repo (does not touch the repo on disk).
/// Rendered unconditionally from `HiveApp::render_ui`, not tied to this
/// panel's own visibility — the owner UX pass made "Tracked repos"
/// Servers-only, but repo removal itself still needs to work from any view.
pub(crate) fn render_remove_confirmation(app: &mut HiveApp, ui: &mut egui::Ui) {
    let ctx = &ui.ctx().clone();
    let Some((path, name)) = app.confirm_remove_repo.clone() else {
        return;
    };
    let mut open = true;
    let mut do_confirm = false;
    let mut do_cancel = false;
    egui::Window::new("Remove repo?")
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            ui.label(
                egui::RichText::new(format!("Stop tracking '{name}' in Switchbard?")).strong(),
            );
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new(format!("Path: {}", path.display())).color(theme::muted_text()),
            );
            ui.add_space(6.0);
            ui.label(
                "This only removes it from Switchbard — the repository and its \
                 worktrees stay on disk untouched.",
            );
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.add(theme::danger_button("Remove")).clicked() {
                    do_confirm = true;
                }
                if ui.button("Cancel").clicked() {
                    do_cancel = true;
                }
            });
        });
    if do_confirm {
        app.remove_repo(path);
        app.confirm_remove_repo = None;
    } else if do_cancel || !open {
        app.confirm_remove_repo = None;
    }
}
