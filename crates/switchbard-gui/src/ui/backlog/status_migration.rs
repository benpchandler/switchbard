//! The standardization offer: "this repo doesn't declare that status — add
//! it?"
//!
//! # Why an offer and not a rule
//!
//! Switchbard used to simply assert the shared vocabulary: every board showed
//! all of `STANDARD_STATUSES` whatever a repo declared. That made the app
//! confident about statuses the `backlog` CLI would refuse to write — a drag
//! to `Icebox` failed with `Invalid status`, and `dispatch`'s release to
//! `In Review` failed silently in three of four repos.
//!
//! The board is now truthful (`ordered_status_vocabulary`), which fixes the
//! lying but reintroduces the original problem: a repo declaring only the
//! trio can't express `In Review`, so the dispatch lifecycle has nowhere to
//! land. This closes that loop by making the repo actually declare it —
//! with the user's consent, once, per repo.
//!
//! Three outcomes, and the third is the one that matters:
//!
//!   - **Standardize** — write the missing statuses into the repo's own
//!     `config.yml`. The vocabulary becomes true rather than assumed.
//!   - **Keep as-is** — recorded in `config.status_standardization_declined`
//!     so it never asks again. The board keeps showing exactly what the repo
//!     declares.
//!   - **Neither yet** — the prompt is not modal-blocking and the board is
//!     already correct without an answer. Nothing is gated on deciding.

use eframe::egui;
use std::path::PathBuf;

use switchbard_core::backlog::status_config::add_standard_statuses;

use crate::app::HiveApp;
use crate::ui::theme;

/// A pending offer for one repo.
#[derive(Debug, Clone)]
pub struct StatusMigrationPrompt {
    pub repo_root: PathBuf,
    pub repo_name: String,
    /// The statuses this repo's config omits, in canonical order.
    pub missing: Vec<String>,
    /// Set when the offer was raised by a refused drop rather than by the
    /// passive check, so the message can say what the user was actually
    /// trying to do instead of describing a config file.
    pub blocked_move: Option<BlockedMove>,
}

#[derive(Debug, Clone)]
pub struct BlockedMove {
    pub task_id: String,
    pub target_status: String,
}

impl StatusMigrationPrompt {
    pub fn headline(&self) -> String {
        match &self.blocked_move {
            Some(m) => format!(
                "{} can't move to \"{}\" — {} doesn't declare that status",
                m.task_id, m.target_status, self.repo_name
            ),
            None => format!(
                "{} declares {} of the {} shared statuses",
                self.repo_name,
                5 - self.missing.len(),
                5
            ),
        }
    }
}

/// A repo's directory name — what the user calls it, not its full path.
pub fn repo_label(repo_root: &std::path::Path) -> String {
    repo_root
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| repo_root.display().to_string())
}

/// The passive half of the check: raise the offer for the first scoped repo
/// with a gap, unless the user already declined it.
///
/// Deliberately does nothing when a prompt is already up (including one
/// raised by a refused drop, which is more specific and shouldn't be
/// overwritten by the generic one) and stops at the first gap — a queue of
/// prompts for eight repos is a wall, not an offer.
pub(super) fn detect(app: &mut HiveApp, roots: &[PathBuf]) {
    if app.status_migration_prompt.is_some() {
        return;
    }
    let declined = app.config.status_standardization_declined.clone();
    let projects = app.backlog_projects.lock().unwrap();
    for root in roots {
        if declined.contains(root) {
            continue;
        }
        let Some(project) = projects.get(root) else {
            continue;
        };
        // An unloaded or CLI-less project has nothing to say yet; asking
        // about a repo we can't read would be guessing.
        if project.configured_statuses.is_empty() {
            continue;
        }
        let missing = switchbard_core::missing_standard_statuses(project);
        if missing.is_empty() {
            continue;
        }
        drop(projects);
        app.status_migration_prompt = Some(StatusMigrationPrompt {
            repo_name: repo_label(root),
            repo_root: root.clone(),
            missing,
            blocked_move: None,
        });
        return;
    }
}

/// Render the offer. Returns nothing; all state changes go through `app`.
pub(super) fn render(app: &mut HiveApp, ui: &mut egui::Ui) {
    let Some(prompt) = app.status_migration_prompt.clone() else {
        return;
    };
    let mut close = false;
    let mut standardize = false;
    let mut decline = false;

    egui::Frame::NONE
        .fill(theme::card_bg())
        .stroke(theme::surface_stroke())
        .inner_margin(egui::Margin::symmetric(10, 8))
        .corner_radius(6.0)
        .show(ui, |ui| {
            ui.label(egui::RichText::new(prompt.headline()).strong());
            ui.label(
                egui::RichText::new(format!(
                    "Add {} to {}/backlog/config.yml? The `backlog` CLI only accepts \
                     statuses that file declares, so until it does, this column isn't \
                     a place {} can go.",
                    prompt.missing.join(" and "),
                    prompt.repo_name,
                    prompt
                        .blocked_move
                        .as_ref()
                        .map_or("its tasks", |_| "this task")
                ))
                .color(theme::muted_text()),
            );
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui.button("Add the missing statuses").clicked() {
                    standardize = true;
                }
                if ui
                    .button(format!("Keep {}'s statuses", prompt.repo_name))
                    .on_hover_text("Stops asking for this repo. The board keeps showing exactly what it declares.")
                    .clicked()
                {
                    decline = true;
                }
                if ui.button("Not now").clicked() {
                    close = true;
                }
            });
        });

    if standardize {
        match add_standard_statuses(&prompt.repo_root) {
            Ok(statuses) => {
                app.backlog_status.set(format!(
                    "{} now declares {}",
                    prompt.repo_name,
                    statuses.join(", ")
                ));
                // The in-memory project still carries the old list, and every
                // status surface reads it — reload so the new column appears
                // in the same frame the user was promised it. A failure here
                // is reported, not swallowed: the file write succeeded, so a
                // silent miss would leave the board still refusing a status
                // the config now allows.
                if let Err(e) = crate::app::refresh_backlog_project_cache(
                    &app.backlog_projects,
                    &prompt.repo_root,
                ) {
                    app.backlog_status.set(format!(
                        "{} updated, but the reload failed: {e}",
                        prompt.repo_name
                    ));
                }
            }
            Err(e) => app
                .backlog_status
                .set(format!("couldn't update {}: {e}", prompt.repo_name)),
        }
        close = true;
    }
    if decline {
        if !app
            .config
            .status_standardization_declined
            .contains(&prompt.repo_root)
        {
            app.config
                .status_standardization_declined
                .push(prompt.repo_root.clone());
            app.save_config();
        }
        close = true;
    }
    if close {
        app.status_migration_prompt = None;
    }
}
