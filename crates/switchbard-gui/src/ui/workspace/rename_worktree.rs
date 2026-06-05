use crate::app::HiveApp;
use crate::runtime::worktree_rename::RenameWorktreeValidationError;
use crate::ui::theme;
use eframe::egui;

pub fn render_modal(app: &mut HiveApp, ctx: &egui::Context) {
    let Some(mut state) = app.rename_worktree_dialog.clone() else {
        return;
    };
    let worktrees = app.worktrees_snapshot();
    let validation = state.validate_with_worktrees(&app.config, &worktrees).err();
    let mut do_confirm = false;
    let mut do_cancel = false;

    egui::Window::new("Rename worktree")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            ui.set_max_width(440.0);
            ui.label(
                egui::RichText::new(state.worktree_path.display().to_string())
                    .color(theme::WEAK_TEXT)
                    .small(),
            );
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.label("Name");
                ui.add(egui::TextEdit::singleline(&mut state.name).desired_width(300.0));
            });

            if let Some(err) = validation.as_ref() {
                ui.add_space(6.0);
                ui.colored_label(theme::DANGER, validation_message(err));
            }
            if let Some(err) = &state.error {
                ui.add_space(6.0);
                ui.colored_label(theme::DANGER, err);
            }

            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button("Cancel").clicked() {
                    do_cancel = true;
                }
                if ui
                    .add_enabled(validation.is_none(), egui::Button::new("Rename"))
                    .clicked()
                {
                    do_confirm = true;
                }
            });
        });

    if do_cancel {
        app.rename_worktree_dialog = None;
        return;
    }

    app.rename_worktree_dialog = Some(state);
    if do_confirm {
        app.execute_rename_worktree();
    }
}

fn validation_message(err: &RenameWorktreeValidationError) -> &'static str {
    err.message()
}
