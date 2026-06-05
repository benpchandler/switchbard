use crate::app::HiveApp;
use crate::runtime::worktree_create::{CreateCheckoutMode, CreateWorktreeValidationError};
use crate::ui::theme;
use eframe::egui;

pub fn render_modal(app: &mut HiveApp, ctx: &egui::Context) {
    let Some(mut state) = app.create_worktree_dialog.lock().unwrap().clone() else {
        return;
    };
    let worktrees = app.worktrees_snapshot();
    let validation = state.validate(&app.config, &worktrees).err();
    let mut do_confirm = false;
    let mut do_cancel = false;

    egui::Window::new("Create worktree")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            ui.set_max_width(620.0);
            ui.label(egui::RichText::new(&state.repo.name).strong());
            ui.add_space(6.0);

            ui.add_enabled_ui(!state.busy, |ui| {
                egui::Grid::new("create_worktree_form")
                    .num_columns(2)
                    .spacing([10.0, 8.0])
                    .show(ui, |ui| {
                        ui.label("Name");
                        let old_name = state.name.clone();
                        if ui
                            .add(egui::TextEdit::singleline(&mut state.name).desired_width(360.0))
                            .changed()
                        {
                            state.sync_defaults_after_name_edit(&old_name);
                        }
                        ui.end_row();

                        ui.label("Location");
                        ui.add(
                            egui::TextEdit::singleline(&mut state.worktree_path)
                                .desired_width(360.0),
                        );
                        ui.end_row();

                        ui.label("Checkout");
                        ui.horizontal(|ui| {
                            ui.radio_value(
                                &mut state.checkout_mode,
                                CreateCheckoutMode::NewBranch,
                                "New branch",
                            );
                            ui.radio_value(
                                &mut state.checkout_mode,
                                CreateCheckoutMode::ExistingBranch,
                                "Existing branch",
                            );
                        });
                        ui.end_row();

                        ui.label("Branch");
                        ui.add(egui::TextEdit::singleline(&mut state.branch).desired_width(360.0));
                        ui.end_row();

                        ui.label("Base");
                        ui.add_enabled(
                            state.checkout_mode == CreateCheckoutMode::NewBranch,
                            egui::TextEdit::singleline(&mut state.base).desired_width(360.0),
                        );
                        ui.end_row();
                    });
            });

            ui.add_space(8.0);
            ui.label(egui::RichText::new("Preview").color(theme::WEAK_TEXT));
            ui.add(
                egui::Label::new(
                    egui::RichText::new(state.command_preview())
                        .monospace()
                        .color(theme::MUTED_TEXT),
                )
                .wrap(),
            );

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
                ui.add_enabled_ui(!state.busy, |ui| {
                    if ui.button("Cancel").clicked() {
                        do_cancel = true;
                    }
                    if ui
                        .add_enabled(validation.is_none(), egui::Button::new("Create worktree"))
                        .clicked()
                    {
                        do_confirm = true;
                    }
                });
                if state.busy {
                    ui.add_space(4.0);
                    ui.spinner();
                    ui.label("creating...");
                }
            });
        });

    if do_cancel {
        app.cancel_create_worktree();
        return;
    }

    *app.create_worktree_dialog.lock().unwrap() = Some(state);
    if do_confirm {
        app.execute_create_worktree(ctx);
    }
}

fn validation_message(err: &CreateWorktreeValidationError) -> &'static str {
    err.message()
}
