//! "Tracked repos" — a left-side panel local to the Servers view (owner UX
//! pass, 2026-08-05: freed up the right edge for the Backlog view's
//! persistent detail rail, `ui::backlog::rail`). Repo add/remove for every
//! *other* view goes through the Settings window (`ui::settings`) instead —
//! this panel's own add/remove buttons still work the same way, just no
//! longer the only way to reach those actions. The per-repo "N listeners"
//! badge is meaningful here specifically because it's now scoped to the one
//! view (Servers) where listener counts are the point.

use crate::app::HiveApp;
use crate::runtime::PickerState;
use crate::ui::theme;
use eframe::egui;
use switchbard_core::WorktreeRef;

/// TASK-27 (owner-requested UX): collapsed to a thin, non-resizable rail —
/// just the toggle button, no repo list — to reclaim horizontal space
/// without losing the panel's presence entirely (a fully-hidden panel would
/// need a separate "where did the repo list go" affordance elsewhere).
const COLLAPSED_WIDTH: f32 = 28.0;

pub fn render(app: &mut HiveApp, ctx: &egui::Context) {
    if app.config.ui.sidebar_collapsed {
        render_collapsed(app, ctx);
        return;
    }

    let repos = app.repos_snapshot();
    let worktrees = app.worktrees_snapshot();
    let picker_busy = matches!(*app.picker.lock().unwrap(), PickerState::InFlight);
    let config_msg = app.config_status.snapshot();

    // User intents queued during the immediate-mode render; applied after the
    // SidePanel closure returns so we don't double-borrow `app`.
    let mut want_pick = false;
    let mut move_request: Option<(usize, isize)> = None;

    egui::SidePanel::left("repos")
        .resizable(true)
        .default_width(280.0)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                if ui
                    .small_button("◀")
                    .on_hover_text("Collapse the tracked-repos panel")
                    .clicked()
                {
                    app.config.ui.sidebar_collapsed = true;
                }
                ui.heading("Tracked repos");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let label = if picker_busy { "Picking…" } else { "➕ Add" };
                    if ui
                        .add_enabled(!picker_busy, egui::Button::new(label))
                        .on_hover_text("Choose a folder containing a git repository")
                        .clicked()
                    {
                        want_pick = true;
                    }
                });
            });
            ui.label(
                egui::RichText::new(format!(
                    "{} repo{} · {} worktree{}",
                    repos.len(),
                    if repos.len() == 1 { "" } else { "s" },
                    worktrees.len(),
                    if worktrees.len() == 1 { "" } else { "s" }
                ))
                .color(theme::muted_text()),
            );
            if let Some(msg) = &config_msg {
                ui.add_space(2.0);
                // TASK-28: same clamped label top_bar.rs's status messages
                // use — defense in depth against unbounded multi-line text,
                // regardless of which surface a Status ends up painted on.
                crate::ui::components::action_status_label(ui, msg, Some(theme::muted_text()));
            }
            ui.add_space(6.0);

            if repos.is_empty() {
                ui.add_space(20.0);
                ui.vertical_centered(|ui| {
                    ui.label(egui::RichText::new("No repos configured yet").strong());
                    ui.add_space(6.0);
                    ui.label(
                        egui::RichText::new(
                            "Click ➕ Add above and pick a folder\nthat contains a git repository.",
                        )
                        .color(theme::muted_text()),
                    );
                });
                return;
            }

            let repo_count_total = repos.len();
            let s = app.state.lock().unwrap();
            for (i, repo) in repos.iter().enumerate() {
                let repo_count = s
                    .listeners
                    .iter()
                    .filter(|l| l.repo_name.as_deref() == Some(repo.name.as_str()))
                    .count();
                let repo_worktrees: Vec<&WorktreeRef> = worktrees
                    .iter()
                    .filter(|w| w.repo_name == repo.name)
                    .collect();
                let expanded = app.expanded_repos.contains(&repo.name);

                ui.horizontal(|ui| {
                    if repo_count > 0 {
                        theme::painted_dot_pulse(ui, theme::green(), repo_count);
                    } else {
                        theme::painted_dot(ui, theme::idle_dot());
                    }
                    if theme::caret_button(ui, expanded).clicked() {
                        if expanded {
                            app.expanded_repos.remove(&repo.name);
                        } else {
                            app.expanded_repos.insert(repo.name.clone());
                        }
                    }
                    // Render right-edge controls first via a right-to-left
                    // sub-layout, then let a nested left-to-right layout fill
                    // the remaining space with the (truncating) repo label.
                    // Without this, the label claims full width and the
                    // right-side widgets draw on top of it.
                    let label = format!("{} ({} wt)", repo.name, repo_worktrees.len());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .add(
                                egui::Button::new(egui::RichText::new("Remove").small())
                                    .frame(false),
                            )
                            .on_hover_text(format!(
                                "Remove '{}' from Switchbard (confirms before removing; \
                                 does not delete the repo on disk)",
                                repo.name
                            ))
                            .clicked()
                        {
                            app.confirm_remove_repo = Some((repo.path.clone(), repo.name.clone()));
                        }
                        let can_down = i + 1 < repo_count_total;
                        if theme::triangle_button(ui, false, can_down)
                            .on_hover_text("Move down")
                            .clicked()
                        {
                            move_request = Some((i, 1));
                        }
                        let can_up = i > 0;
                        if theme::triangle_button(ui, true, can_up)
                            .on_hover_text("Move up")
                            .clicked()
                        {
                            move_request = Some((i, -1));
                        }
                        if repo_count > 0 {
                            ui.label(egui::RichText::new(format!("{repo_count}")).strong());
                        } else {
                            ui.label(egui::RichText::new("—").color(theme::muted_text()));
                        }
                        // Remaining space → label (truncates with ellipsis).
                        ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                            let resp = ui.add(
                                egui::Label::new(label)
                                    .truncate()
                                    .sense(egui::Sense::click()),
                            );
                            if resp.clicked() {
                                if expanded {
                                    app.expanded_repos.remove(&repo.name);
                                } else {
                                    app.expanded_repos.insert(repo.name.clone());
                                }
                            }
                        });
                    });
                });

                if app.expanded_repos.contains(&repo.name) {
                    for w in &repo_worktrees {
                        let n = s
                            .listeners
                            .iter()
                            .filter(|l| l.worktree_path.as_ref() == Some(&w.path))
                            .count();
                        ui.horizontal(|ui| {
                            ui.add_space(18.0);
                            if n > 0 {
                                theme::painted_dot_small_pulse(ui, theme::green(), n);
                            } else {
                                theme::painted_dot_small(ui, theme::idle_dot());
                            }
                            let branch = w.branch.as_deref().unwrap_or("(detached)");
                            ui.label(egui::RichText::new(branch).small());
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if n > 0 {
                                        ui.label(
                                            egui::RichText::new(format!("{n}")).small().strong(),
                                        );
                                    }
                                },
                            );
                        });
                    }
                    ui.add_space(4.0);
                }
            }
        });

    if want_pick {
        app.open_repo_picker(ctx);
    }
    if let Some((i, delta)) = move_request {
        app.move_repo(i, delta);
    }
}

/// The collapsed rail (TASK-27): a fixed-width, non-resizable panel with
/// only the expand toggle.
fn render_collapsed(app: &mut HiveApp, ctx: &egui::Context) {
    egui::SidePanel::left("repos")
        .resizable(false)
        .exact_width(COLLAPSED_WIDTH)
        .show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(4.0);
                if ui
                    .small_button("▶")
                    .on_hover_text("Expand the tracked-repos panel")
                    .clicked()
                {
                    app.config.ui.sidebar_collapsed = false;
                }
            });
        });
}

/// Modal that pops over the whole window when the user clicks "Remove" next
/// to a repo (from either this panel or the Settings window's own repo
/// list). Confirm removes the repo (does not touch the repo on disk).
/// Rendered unconditionally from `HiveApp::render_ui`, not tied to this
/// panel's own visibility — the owner UX pass made "Tracked repos"
/// Servers-only, but repo removal itself still needs to work from any view.
pub(crate) fn render_remove_confirmation(app: &mut HiveApp, ctx: &egui::Context) {
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
