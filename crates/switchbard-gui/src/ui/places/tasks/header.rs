//! TASK-97: a group header row (name + computed roll-up, an expand caret)
//! and its in-place expanded summary band (mock §3) — the project page's
//! replacement per the decision record (Q12 = B: the header expands for
//! summary; the project page itself is cut, no navigation anywhere).

use eframe::egui;

use crate::app::HiveApp;
use crate::ui::components::{status_pill, StatusKind};
use crate::ui::theme;
use switchbard_core::GoalPace;

use super::groups::Group;

/// One group header row: caret, name, `done/total` roll-up, and — `Project`
/// groups only — a status chip when the def declares one.
pub(super) fn render(
    app: &mut HiveApp,
    ui: &mut egui::Ui,
    groups: &[Group<'_>],
    index: usize,
    row_height: f32,
) {
    let group = &groups[index];
    let expanded = app.tasks_place.expanded_groups.contains(&group.key);
    ui.allocate_ui(egui::vec2(ui.available_width(), row_height), |ui| {
        egui::Frame::default()
            .fill(theme::faint_bg())
            .inner_margin(egui::Margin::symmetric(6, 4))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    if theme::caret_button(ui, expanded).clicked() {
                        if expanded {
                            app.tasks_place.expanded_groups.remove(&group.key);
                        } else {
                            app.tasks_place.expanded_groups.insert(group.key.clone());
                        }
                    }
                    ui.label(egui::RichText::new(&group.key).strong());
                    ui.label(
                        egui::RichText::new(format!("{}/{} done", group.done, group.total))
                            .color(theme::muted_text()),
                    );
                    if let Some(status) = &group.status_chip {
                        status_pill(ui, project_status_kind(status), status, None);
                    }
                    if let Some(target) = &group.target_date {
                        ui.label(
                            egui::RichText::new(format!("target {target}"))
                                .small()
                                .color(theme::muted_text()),
                        );
                    }
                });
            });
    });
}

/// The expanded in-place summary band (mock §3): remaining count, a
/// progress meter, a goal-pace chip when a goal counts this group, and the
/// project's description — all in one row, since `list_body`'s
/// virtualization requires every flat-list entry to share [`super::
/// list_body::ROW_HEIGHT`].
pub(super) fn render_summary(ui: &mut egui::Ui, group: &Group<'_>, row_height: f32) {
    ui.allocate_ui(egui::vec2(ui.available_width(), row_height), |ui| {
        egui::Frame::default()
            .fill(theme::faint_bg())
            .inner_margin(egui::Margin::symmetric(6, 4))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    let remaining = group.total.saturating_sub(group.done);
                    ui.label(
                        egui::RichText::new(format!("{remaining} remaining"))
                            .color(theme::muted_text()),
                    );
                    let fraction = if group.total == 0 {
                        0.0
                    } else {
                        group.done as f32 / group.total as f32
                    };
                    // Mock §3's real progress meter, not a dot — colored by
                    // the attached goal's pace when one counts this group
                    // (the same color its pace pill paints below), or the
                    // neutral accent when no goal is attached at all.
                    let meter_color = group
                        .goal
                        .as_ref()
                        .map(|goal| goal_pace_label(goal.pace).0.color())
                        .unwrap_or_else(theme::sky);
                    theme::painted_meter(ui, fraction, meter_color, egui::vec2(120.0, 6.0));
                    if let Some(goal) = &group.goal {
                        let (kind, label) = goal_pace_label(goal.pace);
                        status_pill(
                            ui,
                            kind,
                            format!("goal: {label} · {}/{}", goal.actual, goal.target),
                            None,
                        );
                    }
                    if !group.description.is_empty() {
                        ui.label(
                            egui::RichText::new(&group.description)
                                .small()
                                .color(theme::muted_text()),
                        );
                    }
                });
            });
    });
}

fn goal_pace_label(pace: GoalPace) -> (StatusKind, &'static str) {
    match pace {
        GoalPace::OnTrack => (StatusKind::Good, "on track"),
        GoalPace::Behind => (StatusKind::Warn, "behind"),
        GoalPace::Met => (StatusKind::Good, "met"),
        GoalPace::Missed => (StatusKind::Danger, "missed"),
    }
}

/// Mirrors `ui::backlog::projects::project_status_kind` — duplicated
/// rather than imported: that module renders the now-orphaned Projects
/// lens (still compiling, not reachable from Tasks per the binding
/// directive), and this is a six-line presentational mapping, not a shared
/// invariant worth a cross-module dependency for.
fn project_status_kind(status: &str) -> StatusKind {
    if status.eq_ignore_ascii_case("completed") {
        StatusKind::Good
    } else if status.eq_ignore_ascii_case("in progress") {
        StatusKind::Info
    } else if status.eq_ignore_ascii_case("canceled") {
        StatusKind::Warn
    } else {
        StatusKind::Neutral
    }
}
