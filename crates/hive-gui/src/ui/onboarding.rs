//! First-launch onboarding modal.
//!
//! Shown when `config.ui.onboarding_dismissed == false` AND
//! `config.repos.is_empty()`. Auto-scans common dev directories (`~/Dev`,
//! `~/code`, `~/src`, `~/Projects`, …) for git repositories, presents them
//! as a checklist, and adds the selected ones in one click.
//!
//! Three exits, each dismisses the modal permanently:
//!   - **Add N selected** — adds checked rows, marks dismissed.
//!   - **Browse for a folder…** — opens the existing file picker; first
//!     successful add marks dismissed (see `app.rs::add_repo_from_path`).
//!   - **Skip — I'll add later** — marks dismissed, no repos added.
//!
//! Dismissal is one-way: removing all repos later does NOT re-open this.
//! Re-onboarding would be more annoying than the empty-state sidebar
//! already is.
//!
//! ### Default-selected heuristic
//! Repos modified within the last 90 days are pre-checked. The reasoning:
//! a repo you haven't touched in 3 months is probably reference / archive,
//! not active work. The user can flip the checkbox if they disagree.

use crate::app::HiveApp;
use crate::ui::theme;
use eframe::egui;
use hive_core::{auto_scan_roots, discover_repos, DiscoveredRepo};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, SystemTime};

/// 90-day recency cutoff for the default-select heuristic.
const RECENT_CUTOFF_DAYS: u64 = 90;

#[derive(Debug, Clone)]
pub enum DiscoveryState {
    /// Modal is closed (user dismissed or never needed it).
    Hidden,
    /// Background scan in progress; modal shows a spinner.
    Scanning,
    /// Scan complete; rows are the discovered repos plus per-row checkbox
    /// state. Empty `rows` is a real outcome (no Dev folder, fresh Mac):
    /// the modal shows a "Nothing found — browse for a folder" pane.
    Ready { rows: Vec<OnboardingRow> },
}

impl Default for DiscoveryState {
    fn default() -> Self {
        Self::Hidden
    }
}

#[derive(Debug, Clone)]
pub struct OnboardingRow {
    pub repo: DiscoveredRepo,
    pub selected: bool,
}

/// Decide whether to show the modal on this update tick. Called from the
/// app's update loop. Returns true iff the caller should render us this
/// frame.
pub fn should_show(app: &HiveApp) -> bool {
    !app.config.ui.onboarding_dismissed && app.config.repos.is_empty()
}

/// Kick off the discovery scan on a background thread. Called once when
/// the modal transitions from Hidden → Scanning so we don't block the UI
/// thread on filesystem walks.
pub fn start_discovery(state: Arc<Mutex<DiscoveryState>>, ctx: egui::Context) {
    *state.lock().unwrap() = DiscoveryState::Scanning;
    thread::spawn(move || {
        let home = match dirs::home_dir() {
            Some(h) => h,
            None => {
                *state.lock().unwrap() = DiscoveryState::Ready { rows: vec![] };
                ctx.request_repaint();
                return;
            }
        };
        let roots = auto_scan_roots(&home);
        let discovered = discover_repos(&roots);
        let rows = discovered
            .into_iter()
            .map(|repo| {
                let selected = is_recent(&repo, RECENT_CUTOFF_DAYS);
                OnboardingRow { repo, selected }
            })
            .collect();
        *state.lock().unwrap() = DiscoveryState::Ready { rows };
        ctx.request_repaint();
    });
}

fn is_recent(repo: &DiscoveredRepo, cutoff_days: u64) -> bool {
    let cutoff = Duration::from_secs(cutoff_days * 24 * 60 * 60);
    match SystemTime::now().duration_since(repo.modified) {
        Ok(age) => age <= cutoff,
        // Modified time is in the future (clock skew, network share);
        // treat as recent.
        Err(_) => true,
    }
}

/// Format a "touched Nd ago" hint for the row. Mirrors `humanize_age` but
/// from a SystemTime input.
fn recency_hint(repo: &DiscoveredRepo) -> String {
    let now = SystemTime::now();
    let secs = now
        .duration_since(repo.modified)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let days = secs / 86_400;
    if days == 0 {
        "touched today".to_string()
    } else if days == 1 {
        "touched 1d ago".to_string()
    } else if days < 30 {
        format!("touched {days}d ago")
    } else if days < 365 {
        format!("touched {}mo ago", days / 30)
    } else {
        format!("touched {}y ago", days / 365)
    }
}

/// Truncate a path to `~/...` for display. Falls back to full display on
/// any failure.
fn display_path(path: &PathBuf, home: &PathBuf) -> String {
    if let Ok(rel) = path.strip_prefix(home) {
        format!("~/{}", rel.display())
    } else {
        path.display().to_string()
    }
}

/// Pending intents emitted by the modal during a render. Applied after
/// the show() closure exits so we don't try to mutate `app` while the
/// `Window` still borrows the context.
#[derive(Default)]
struct Pending {
    add: Vec<PathBuf>,
    browse: bool,
    dismiss: bool,
}

/// Render the modal. Returns immediately if `should_show` says no.
pub fn render(app: &mut HiveApp, ctx: &egui::Context) {
    if !should_show(app) {
        return;
    }

    // Lazy-init the discovery scan the first frame the modal becomes
    // visible. Stays on app instance for subsequent re-renders.
    let mut starting_fresh = false;
    {
        let mut guard = app.onboarding.lock().unwrap();
        if matches!(*guard, DiscoveryState::Hidden) {
            *guard = DiscoveryState::Scanning;
            starting_fresh = true;
        }
    }
    if starting_fresh {
        start_discovery(app.onboarding.clone(), ctx.clone());
    }

    let snapshot = app.onboarding.lock().unwrap().clone();
    let mut pending = Pending::default();

    // Modal-ish presentation: dim the background so the user knows this
    // is the moment to engage. egui doesn't have a true modal primitive,
    // but a fullscreen painted overlay plus a centered Window is the
    // idiomatic approximation.
    let screen_rect = ctx.screen_rect();
    egui::Area::new(egui::Id::new("onboarding-scrim"))
        .order(egui::Order::Background)
        .fixed_pos(screen_rect.min)
        .show(ctx, |ui| {
            ui.painter().rect_filled(
                screen_rect,
                0.0,
                egui::Color32::from_black_alpha(120),
            );
        });

    egui::Window::new("Welcome to Hive")
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .collapsible(false)
        .resizable(false)
        .default_width(560.0)
        .show(ctx, |ui| {
            ui.label(
                egui::RichText::new(
                    "Let's set up your workspace.",
                )
                .strong(),
            );
            ui.add_space(6.0);

            match &snapshot {
                DiscoveryState::Hidden => {
                    // Should never render in this state, but be safe.
                    ui.spinner();
                }
                DiscoveryState::Scanning => {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label("Scanning your dev directories…");
                    });
                    ui.add_space(8.0);
                    ui.label(
                        egui::RichText::new(
                            "Looking under ~/ for folders that contain git repositories.",
                        )
                        .weak(),
                    );
                }
                DiscoveryState::Ready { rows } if rows.is_empty() => {
                    render_empty_pane(ui, &mut pending);
                }
                DiscoveryState::Ready { rows } => {
                    render_picklist(ui, rows, &mut pending, app);
                }
            }

            ui.add_space(10.0);
            ui.label(
                egui::RichText::new(
                    "Hive only watches what you add. Nothing leaves your machine.",
                )
                .weak()
                .small(),
            );
        });

    // Apply pending intents after the window closes so we're not double-
    // borrowing the context / app state. Checkbox flips are written
    // through `apply_one`/`apply_all` synchronously inside the render
    // (the shared `Mutex<DiscoveryState>` is the source of truth, and
    // egui's borrow checker is fine with that since we re-read it next
    // frame).
    if !pending.add.is_empty() {
        for path in &pending.add {
            app.add_repo_from_path(path.clone());
        }
        dismiss(app);
    }
    if pending.browse {
        app.open_repo_picker(ctx);
        // The picker may yield Idle (user canceled). Don't dismiss until
        // an actual add comes through — the modal stays put, ready for
        // another attempt. The add path itself dismisses on success.
    }
    if pending.dismiss {
        dismiss(app);
    }
}

fn render_picklist(
    ui: &mut egui::Ui,
    rows: &[OnboardingRow],
    pending: &mut Pending,
    app: &HiveApp,
) {
    let home = dirs::home_dir().unwrap_or_default();
    let total = rows.len();
    let selected_count = rows.iter().filter(|r| r.selected).count();

    ui.label(
        egui::RichText::new(format!(
            "Found {total} git repositor{}. Pick the ones you want to track:",
            if total == 1 { "y" } else { "ies" }
        ))
        .weak(),
    );
    ui.add_space(6.0);

    // Toggle-all affordance — quietly visible above the list. Saves clicks
    // when the heuristic guessed wrong on which 80% are "recent".
    ui.horizontal(|ui| {
        if ui
            .small_button("Select all")
            .clicked()
        {
            apply_all(&app.onboarding, true);
        }
        if ui
            .small_button("Select none")
            .clicked()
        {
            apply_all(&app.onboarding, false);
        }
    });
    ui.add_space(4.0);

    egui::ScrollArea::vertical()
        .max_height(360.0)
        .auto_shrink([false, true])
        .show(ui, |ui| {
            for row in rows {
                ui.horizontal(|ui| {
                    let mut selected = row.selected;
                    if ui.checkbox(&mut selected, "").changed() {
                        apply_one(&app.onboarding, &row.repo.path, selected);
                    }
                    ui.vertical(|ui| {
                        ui.label(egui::RichText::new(&row.repo.name).strong());
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new(display_path(&row.repo.path, &home))
                                    .weak()
                                    .small(),
                            );
                            ui.label(
                                egui::RichText::new(format!("· {}", recency_hint(&row.repo)))
                                    .weak()
                                    .small(),
                            );
                        });
                    });
                });
                ui.add_space(2.0);
            }
        });

    ui.add_space(10.0);
    ui.horizontal(|ui| {
        let add_label = if selected_count == 0 {
            "Add selected".to_string()
        } else {
            format!("Add {selected_count} selected")
        };
        if ui
            .add_enabled(
                selected_count > 0,
                egui::Button::new(egui::RichText::new(add_label).color(egui::Color32::WHITE))
                    .fill(theme::GREEN),
            )
            .clicked()
        {
            for row in rows.iter().filter(|r| r.selected) {
                pending.add.push(row.repo.path.clone());
            }
        }
        if ui.button("Browse for a folder…").clicked() {
            pending.browse = true;
        }
        if ui.button("Skip — I'll add later").clicked() {
            pending.dismiss = true;
        }
    });
}

fn render_empty_pane(ui: &mut egui::Ui, pending: &mut Pending) {
    ui.label(
        egui::RichText::new(
            "We didn't find any git repositories in the usual places.",
        )
        .weak(),
    );
    ui.add_space(6.0);
    ui.label(
        egui::RichText::new(
            "Pick a folder containing a git repository to get started.",
        )
        .weak()
        .small(),
    );
    ui.add_space(10.0);
    ui.horizontal(|ui| {
        if ui
            .add(
                egui::Button::new(
                    egui::RichText::new("Browse for a folder…").color(egui::Color32::WHITE),
                )
                .fill(theme::GREEN),
            )
            .clicked()
        {
            pending.browse = true;
        }
        if ui.button("Skip — I'll add later").clicked() {
            pending.dismiss = true;
        }
    });
}

fn apply_all(state: &Arc<Mutex<DiscoveryState>>, selected: bool) {
    if let DiscoveryState::Ready { rows } = &mut *state.lock().unwrap() {
        for row in rows.iter_mut() {
            row.selected = selected;
        }
    }
}

fn apply_one(state: &Arc<Mutex<DiscoveryState>>, path: &PathBuf, selected: bool) {
    if let DiscoveryState::Ready { rows } = &mut *state.lock().unwrap() {
        for row in rows.iter_mut() {
            if row.repo.path == *path {
                row.selected = selected;
            }
        }
    }
}

/// Mark the modal permanently dismissed and persist. Called from every
/// exit path: Add Selected, Skip, and `add_repo_from_path` once the
/// browse-flow user picks a repo.
pub fn dismiss(app: &mut HiveApp) {
    app.config.ui.onboarding_dismissed = true;
    app.save_config();
    *app.onboarding.lock().unwrap() = DiscoveryState::Hidden;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, SystemTime};

    fn repo(name: &str, age: Duration) -> DiscoveredRepo {
        DiscoveredRepo {
            path: PathBuf::from(format!("/Users/dev/{name}")),
            name: name.into(),
            modified: SystemTime::now().checked_sub(age).unwrap(),
        }
    }

    #[test]
    fn recency_hint_buckets() {
        assert_eq!(recency_hint(&repo("a", Duration::from_secs(0))), "touched today");
        assert_eq!(
            recency_hint(&repo("a", Duration::from_secs(86_400))),
            "touched 1d ago"
        );
        assert_eq!(
            recency_hint(&repo("a", Duration::from_secs(5 * 86_400))),
            "touched 5d ago"
        );
        assert_eq!(
            recency_hint(&repo("a", Duration::from_secs(45 * 86_400))),
            "touched 1mo ago"
        );
        assert_eq!(
            recency_hint(&repo("a", Duration::from_secs(2 * 365 * 86_400))),
            "touched 2y ago"
        );
    }

    #[test]
    fn is_recent_within_90_days() {
        assert!(is_recent(&repo("a", Duration::from_secs(30 * 86_400)), 90));
        assert!(is_recent(&repo("a", Duration::from_secs(89 * 86_400)), 90));
        assert!(!is_recent(
            &repo("a", Duration::from_secs(120 * 86_400)),
            90
        ));
    }

    #[test]
    fn future_mtime_counts_as_recent() {
        // Clock skew / shared filesystem: a repo with mtime in the future
        // should default-select rather than vanish.
        let r = DiscoveredRepo {
            path: PathBuf::from("/x"),
            name: "x".into(),
            modified: SystemTime::now() + Duration::from_secs(3600),
        };
        assert!(is_recent(&r, 90));
    }

    #[test]
    fn display_path_collapses_home() {
        let home = PathBuf::from("/Users/dev");
        assert_eq!(
            display_path(&PathBuf::from("/Users/dev/code/alpha"), &home),
            "~/code/alpha"
        );
        // A path outside the home dir falls back to the absolute path.
        assert_eq!(
            display_path(&PathBuf::from("/opt/projects/beta"), &home),
            "/opt/projects/beta"
        );
    }
}
