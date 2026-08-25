//! The summary line ("N open · M total", warnings) and the filter bar
//! (project/scope picker, status/priority filters, completed/archived
//! toggles).

use super::{create, format, reset_task_selection, sort, Pending, Snapshot};
use crate::app::HiveApp;
use crate::runtime::BacklogLens;
use crate::ui::components::{status_pill, StatusKind};
use crate::ui::theme;
use eframe::egui;
use std::collections::BTreeMap;
use std::path::PathBuf;
use switchbard_core::{ordered_status_vocabulary, BacklogTaskSource, BACKLOG_PRIORITIES};

/// The lens tab strip shown under the summary line: List / Board /
/// Milestones / Statistics. Switching lenses does not clear the current
/// filters or selection — every lens reads the same `Snapshot`. Owner UX
/// pass (2026-08-05): wrapped in its own `nav_bg()` band, matching the
/// top bar's view-tab strip, so navigation reads as its own zone rather
/// than blending into the toolbar/content around it.
/// The lens tabs. Renders bare: `render_toolbar_group` owns the container
/// these sit in, so tabs, filters, and saved views read as one control
/// surface instead of three competing treatments (a bordered tab strip, an
/// unframed saved-views row, and a second bordered filter panel).
pub(super) fn render_lens_tabs(app: &mut HiveApp, ui: &mut egui::Ui) {
    {
        ui.horizontal(|ui| {
            for lens in [
                BacklogLens::Digest,
                BacklogLens::List,
                BacklogLens::Board,
                BacklogLens::Milestones,
                BacklogLens::Portfolio,
                BacklogLens::Statistics,
            ] {
                if ui
                    .selectable_label(app.backlog_view.lens == lens, lens.label())
                    .clicked()
                {
                    app.backlog_view.lens = lens;
                }
            }
        });
    }
}

/// The summary line. `visible_count` is the number of tasks the current
/// filters leave — `None` for a lens that does not filter (Digest,
/// Statistics), where "N of M" would be a claim about nothing.
pub(super) fn render_summary(
    app: &mut HiveApp,
    ui: &mut egui::Ui,
    snap: &Snapshot,
    pending: &mut Pending,
    visible_count: Option<usize>,
) {
    let scoped = super::scoped_projects(app, snap);
    let task_count: usize = scoped.iter().map(|row| row.project.tasks.len()).sum();
    let open_count: usize = scoped
        .iter()
        .map(|row| sort::open_task_count(&row.project))
        .sum();
    let warning_count: usize = scoped.iter().map(|row| row.project.warnings.len()).sum();
    let ordering_warning = app.ordering_snapshot().warning;

    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("Backlog").heading().strong());
        ui.separator();
        // One count, and when a filter is narrowing the view it explains the
        // gap itself ("370 of 1509") rather than sitting next to a second,
        // unrelated number further down the toolbar. The scope is not
        // repeated here — the project picker directly below already states
        // it, and saying "All projects" twice was the loudest redundancy in
        // this header.
        let count_label = match visible_count {
            Some(visible) if visible != task_count => {
                format!("{visible} of {task_count} · {open_count} open")
            }
            _ => format!("{task_count} tasks · {open_count} open"),
        };
        ui.label(egui::RichText::new(count_label).color(theme::weak_text()));
        if warning_count > 0 {
            ui.separator();
            status_pill(
                ui,
                StatusKind::Warn,
                format!(
                    "{warning_count} warning{}",
                    if warning_count == 1 { "" } else { "s" }
                ),
                Some("One or more Backlog projects loaded with warnings"),
            );
        }
        if let Some(warning) = &ordering_warning {
            ui.separator();
            status_pill(ui, StatusKind::Warn, "ordering.yml", Some(warning.as_str()));
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .button("Refresh Backlog")
                .on_hover_text("Reload Backlog tasks from tracked worktrees")
                .clicked()
            {
                app.backlog_kick.notify();
                app.backlog_status.set("refreshing Backlog projects");
            }
            if ui
                .button("+ Task")
                .on_hover_text("Create a task in a Backlog project")
                .clicked()
            {
                let target = app
                    .backlog_view
                    .selected_project
                    .clone()
                    .or_else(|| snap.projects.first().map(|row| row.key.clone()));
                create::open_new_task(app, target, None);
            }

            render_cleanup_button(app, ui, snap, pending);
        });
    });
}

/// "Clean Up Old Tasks" (QA parity matrix LOW gap): complete every Done,
/// active, CLI-editable task across every tracked project — a
/// workspace-wide housekeeping action, so it lives in the always-visible
/// summary line rather than the List lens's own toolbar, and always spans
/// every project regardless of the current filter/scope ("cross-repo
/// aware" per the mission's own framing). Confirm-gated the same way
/// Archive/Complete is on a single task: bulk-completing is consequential
/// enough to confirm.
///
/// "Complete", not "Archive" — Backlog.md semantics (verified against a
/// real fixture repo, both `backlog task complete --help` and the CLI's own
/// refusal message): a Done task is *completed* into `backlog/completed/`,
/// not archived into `backlog/archive/`; the real CLI rejects `task
/// archive` on a Done task outright. The button keeps the "Clean Up Old
/// Tasks" name (still an accurate description of the outcome — these tasks
/// leave the active view) even though the underlying CLI verb and
/// resulting `BacklogTaskSource` are Complete/`Completed`, not
/// Archive/`Archived`.
fn render_cleanup_button(
    app: &mut HiveApp,
    ui: &mut egui::Ui,
    snap: &Snapshot,
    pending: &mut Pending,
) {
    let candidates = cleanup_candidates(snap);
    let total: usize = candidates.iter().map(|(_, ids)| ids.len()).sum();

    if app.backlog_view.cleanup_confirm {
        ui.colored_label(theme::amber(), format!("Complete {total} Done tasks?"));
        if ui.add(theme::danger_button("Confirm cleanup")).clicked() {
            pending.cleanup = Some(candidates);
            app.backlog_view.cleanup_confirm = false;
            app.backlog_status
                .set(format!("cleaning up {total} Done tasks"));
        }
        if ui.button("Cancel").clicked() {
            app.backlog_view.cleanup_confirm = false;
        }
    } else if ui
        .add_enabled(total > 0, egui::Button::new("Clean Up Old Tasks"))
        .on_hover_text("Complete every Done task across all tracked projects")
        .clicked()
    {
        app.backlog_view.cleanup_confirm = true;
    }
}

/// Every project's Done, still-active, CLI-editable task ids — the
/// candidate set `render_cleanup_button` completes. A `Completed`-sourced
/// task (already moved to `backlog/completed/`) or an already-
/// `Archived`/`Draft` one is excluded the same way a single task's
/// Archive/Complete button already requires `editable()`.
fn cleanup_candidates(snap: &Snapshot) -> Vec<(PathBuf, Vec<String>)> {
    snap.projects
        .iter()
        .filter(|row| row.project.cli_available())
        .filter_map(|row| {
            let ids: Vec<String> = row
                .project
                .tasks
                .iter()
                .filter(|task| task.editable() && task.is_done())
                .map(|task| task.id.clone())
                .collect();
            (!ids.is_empty()).then_some((row.key.clone(), ids))
        })
        .collect()
}

/// The filter controls. Like `render_lens_tabs`, renders bare inside the
/// container `render_toolbar_group` provides.
/// The visible, archivable tasks grouped by project root.
///
/// Deliberately built from `sort::visible_task_rows` — the *same* function
/// the lens renders from — so "archive what's showing" cannot drift from
/// what is actually on screen. Excludes:
///
/// - **Done tasks**, which the real CLI refuses to archive (they must be
///   completed instead, see `spawn_backlog_cleanup`); including them would
///   half-fail the batch;
/// - tasks already in `archive/` or `completed/`, which have nowhere to go;
/// - projects whose `backlog` CLI is missing, since every write goes
///   through it.
pub(super) fn bulk_archive_candidates(
    app: &HiveApp,
    snap: &Snapshot,
) -> Vec<(PathBuf, Vec<String>)> {
    let mut per_project: BTreeMap<PathBuf, Vec<String>> = BTreeMap::new();
    for row in sort::visible_task_rows(app, snap) {
        if !row.project.project.cli_available() {
            continue;
        }
        if row.task.is_done()
            || matches!(
                row.task.source,
                BacklogTaskSource::Archived | BacklogTaskSource::Completed
            )
        {
            continue;
        }
        per_project
            .entry(row.project.key.clone())
            .or_default()
            .push(row.task.id.clone());
    }
    per_project.into_iter().collect()
}

/// "Archive N showing" — the bulk counterpart to the per-task Archive in the
/// detail rail.
///
/// Only offered when the visible set is a strict subset of the scope. With
/// no filter narrowing anything, "archive what's showing" means "archive the
/// entire backlog", which is not an action anyone means to take from a
/// toolbar button. Archiving is recoverable (files move to
/// `backlog/archive/tasks/`, they are not deleted) but a thousand accidental
/// moves across a dozen repos is still a bad afternoon.
fn render_bulk_archive_button(
    app: &mut HiveApp,
    ui: &mut egui::Ui,
    snap: &Snapshot,
    pending: &mut Pending,
) {
    let scope_total: usize = super::scoped_projects(app, snap)
        .iter()
        .map(|row| row.project.tasks.len())
        .sum();
    let candidates = bulk_archive_candidates(app, snap);
    let total: usize = candidates.iter().map(|(_, ids)| ids.len()).sum();
    let narrowed = total < scope_total;

    if app.backlog_view.bulk_archive_confirm {
        ui.colored_label(theme::amber(), format!("Archive {total} shown tasks?"));
        if ui.add(theme::danger_button("Confirm archive")).clicked() {
            pending.bulk_archive = Some(candidates);
            app.backlog_view.bulk_archive_confirm = false;
            app.backlog_status.set(format!("archiving {total} tasks"));
        }
        if ui.button("Cancel").clicked() {
            app.backlog_view.bulk_archive_confirm = false;
        }
    } else if ui
        .add_enabled(
            total > 0 && narrowed,
            egui::Button::new(format!("Archive {total} showing")),
        )
        .on_hover_text(if narrowed {
            "Move every task currently shown into backlog/archive/tasks (Done tasks are skipped)"
        } else {
            "Narrow the view with a filter first — this archives everything currently shown"
        })
        .clicked()
    {
        app.backlog_view.bulk_archive_confirm = true;
    }
}

pub(super) fn render_project_toolbar(
    app: &mut HiveApp,
    ui: &mut egui::Ui,
    snap: &Snapshot,
    pending: &mut Pending,
) {
    {
        let compact = ui.available_width() < 640.0;
        let project_filter_width = if compact { 140.0 } else { 180.0 };
        let project_picker_width = if compact { 160.0 } else { 280.0 };
        ui.horizontal_wrapped(|ui| {
            ui.label(egui::RichText::new("Project").color(theme::muted_text()));
            ui.add(
                egui::TextEdit::singleline(&mut app.backlog_view.project_filter)
                    .hint_text("Filter projects")
                    .desired_width(project_filter_width),
            );
            let project_filter_lc = app.backlog_view.project_filter.to_lowercase();
            let combo_label = app
                .backlog_view
                .selected_project
                .as_deref()
                .and_then(|key| snap.project(key))
                .map(|row| row.label())
                .unwrap_or_else(|| "All projects".to_string());
            egui::ComboBox::from_id_salt("backlog_project_picker")
                .selected_text(combo_label)
                .width(project_picker_width)
                .show_ui(ui, |ui| {
                    let mut shown = 0usize;
                    if project_filter_lc.is_empty() || "all projects".contains(&project_filter_lc) {
                        shown += 1;
                        let selected = app.backlog_view.selected_project.is_none();
                        let total_open: usize = snap
                            .projects
                            .iter()
                            .map(|row| sort::open_task_count(&row.project))
                            .sum();
                        if ui
                            .selectable_label(
                                selected,
                                format!("All projects  ·  {total_open} open"),
                            )
                            .clicked()
                        {
                            app.backlog_view.selected_project = None;
                            reset_task_selection(app);
                        }
                    }
                    for row in &snap.projects {
                        if !row.matches_filter(&project_filter_lc) {
                            continue;
                        }
                        shown += 1;
                        let selected =
                            app.backlog_view.selected_project.as_deref() == Some(&row.key);
                        let label = format!(
                            "{}  ·  {} open",
                            row.label(),
                            sort::open_task_count(&row.project)
                        );
                        if ui.selectable_label(selected, label).clicked() {
                            app.backlog_view.selected_project = Some(row.key.clone());
                            reset_task_selection(app);
                        }
                    }
                    if shown == 0 {
                        ui.label(
                            egui::RichText::new("No matching projects").color(theme::muted_text()),
                        );
                    }
                });

            if compact {
                ui.end_row();
            }

            ui.separator();
            ui.label(egui::RichText::new("Status").color(theme::muted_text()));
            // Owner UX pass (2026-08-05): the same shared vocabulary Board's
            // columns, the detail-pane editor, and Statistics all consume now,
            // so this dropdown can no longer offer a different status set than
            // what Board actually shows (previously this used a local union
            // that omitted a project's declared-but-currently-empty statuses).
            let scoped = super::scoped_projects(app, snap);
            let statuses = ordered_status_vocabulary(scoped.iter().map(|row| &row.project));
            egui::ComboBox::from_id_salt("backlog_status_filter")
                .selected_text(format::value_filter_label(&app.backlog_view.status_filter))
                .show_ui(ui, |ui| {
                    ui.selectable_value(
                        &mut app.backlog_view.status_filter,
                        "all".to_string(),
                        "All",
                    );
                    for status in statuses {
                        ui.selectable_value(
                            &mut app.backlog_view.status_filter,
                            status.clone(),
                            status,
                        );
                    }
                });

            ui.label(egui::RichText::new("Priority").color(theme::muted_text()));
            egui::ComboBox::from_id_salt("backlog_priority_filter")
                .selected_text(format::priority_filter_label(
                    &app.backlog_view.priority_filter,
                ))
                .show_ui(ui, |ui| {
                    ui.selectable_value(
                        &mut app.backlog_view.priority_filter,
                        "all".to_string(),
                        "All",
                    );
                    for priority in BACKLOG_PRIORITIES {
                        ui.selectable_value(
                            &mut app.backlog_view.priority_filter,
                            (*priority).to_string(),
                            format::priority_title(priority),
                        );
                    }
                });

            if compact {
                ui.end_row();
            }

            ui.label(egui::RichText::new("Milestone").color(theme::muted_text()));
            // Both option lists are built here, before any combo can
            // mutate `app`: they borrow it immutably via `ActiveFilters`,
            // and each `selectable_value` below needs it mutably.
            let (milestones, labels) = {
                let facet_filter_lc = app.filter.to_lowercase();
                let filters = sort::ActiveFilters::from_app(app, &facet_filter_lc);
                let scoped = super::scoped_projects(app, snap);
                (
                    sort::milestone_options(&scoped, &filters, &app.backlog_view.milestone_filter),
                    sort::label_options(&scoped, &filters, &app.backlog_view.label_filter),
                )
            };
            egui::ComboBox::from_id_salt("backlog_milestone_filter")
                .selected_text(format::value_filter_label(
                    &app.backlog_view.milestone_filter,
                ))
                .show_ui(ui, |ui| {
                    ui.selectable_value(
                        &mut app.backlog_view.milestone_filter,
                        "all".to_string(),
                        "All",
                    );
                    for option in milestones {
                        let label = format!("{}  ({})", option.value, option.count);
                        ui.selectable_value(
                            &mut app.backlog_view.milestone_filter,
                            option.value,
                            label,
                        );
                    }
                });

            ui.label(egui::RichText::new("Label").color(theme::muted_text()));
            egui::ComboBox::from_id_salt("backlog_label_filter")
                .selected_text(format::value_filter_label(&app.backlog_view.label_filter))
                .show_ui(ui, |ui| {
                    ui.selectable_value(
                        &mut app.backlog_view.label_filter,
                        "all".to_string(),
                        "All",
                    );
                    for option in labels {
                        let label = format!("{}  ({})", option.value, option.count);
                        ui.selectable_value(
                            &mut app.backlog_view.label_filter,
                            option.value,
                            label,
                        );
                    }
                });

            if compact {
                ui.end_row();
            }

            ui.checkbox(&mut app.backlog_view.show_completed, "Done");
            ui.checkbox(&mut app.backlog_view.show_archived, "Archived");
            ui.checkbox(&mut app.backlog_view.show_drafts, "Drafts");
            ui.separator();
            let stale_days = app.config.ui.stale_after_days;
            if ui
                .checkbox(&mut app.backlog_view.stale_only, "Stale only")
                .on_hover_text(format!(
                    "Show only tasks untouched for {stale_days}+ days (by updated date, falling back to created)"
                ))
                .changed()
            {
                // Changing what is visible must disarm a primed bulk archive:
                // its confirm names a count taken from the filtered set, and
                // that set has just moved underneath it.
                app.backlog_view.bulk_archive_confirm = false;
            }
            let mut days = app.config.ui.stale_after_days;
            if ui
                .add(
                    egui::DragValue::new(&mut days)
                        .speed(1.0)
                        .range(1..=3650)
                        .suffix(" days"),
                )
                .on_hover_text("How long without an update counts as stale")
                .changed()
            {
                app.config.ui.stale_after_days = days;
                app.backlog_view.bulk_archive_confirm = false;
                app.save_config();
            }
            ui.separator();
            render_bulk_archive_button(app, ui, snap, pending);
        });
    }
}
