//! The Milestones lens: every visible task grouped by `BacklogTask::milestone`,
//! cross-repo, sorted alphabetically with an "Unassigned" bucket last
//! (task-15 AC #6). Milestone *assignment* lives in the detail pane
//! (`detail::render_editor`); this lens is read/browse-only — clicking a
//! task selects it so switching back to the List lens opens it in the
//! detail pane.

use super::{format, Snapshot, TaskRow};
use crate::app::HiveApp;
use crate::ui::components::status_pill;
use crate::ui::theme;
use eframe::egui;
use std::collections::BTreeMap;

const UNASSIGNED_LABEL: &str = "Unassigned";

pub(super) fn render_milestones(
    app: &mut HiveApp,
    ui: &mut egui::Ui,
    _snap: &Snapshot,
    tasks: Vec<TaskRow<'_>>,
) {
    let show_repo = app.backlog_view.selected_project.is_none();
    let mut groups: BTreeMap<String, Vec<&TaskRow<'_>>> = BTreeMap::new();
    for row in &tasks {
        let label = row
            .task
            .milestone
            .clone()
            .unwrap_or_else(|| UNASSIGNED_LABEL.to_string());
        groups.entry(label).or_default().push(row);
    }

    egui::ScrollArea::vertical()
        .id_salt("backlog_milestones")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            if groups.is_empty() {
                ui.add_space(20.0);
                ui.label(egui::RichText::new("No tasks match the current filters").strong());
                return;
            }
            // `groups` is a `BTreeMap`, so iterating its keys is already
            // alphabetical; "Unassigned" is rendered separately, last,
            // regardless of where it would otherwise sort — it's the
            // catch-all, not a milestone anyone is tracking progress toward.
            for milestone in groups.keys().cloned().collect::<Vec<_>>() {
                if milestone == UNASSIGNED_LABEL {
                    continue;
                }
                render_group(app, ui, &milestone, &groups[&milestone], show_repo);
            }
            if let Some(unassigned) = groups.get(UNASSIGNED_LABEL) {
                render_group(app, ui, UNASSIGNED_LABEL, unassigned, show_repo);
            }
        });
}

fn render_group(
    app: &mut HiveApp,
    ui: &mut egui::Ui,
    milestone: &str,
    rows: &[&TaskRow<'_>],
    show_repo: bool,
) {
    let done = rows.iter().filter(|row| row.task.is_done()).count();
    let total = rows.len();
    egui::CollapsingHeader::new(format!("{milestone}  ·  {done}/{total} done"))
        .default_open(true)
        .id_salt(format!("milestone_{milestone}"))
        .show(ui, |ui| {
            for row in rows {
                render_row(app, ui, row, show_repo);
            }
        });
}

fn render_row(app: &mut HiveApp, ui: &mut egui::Ui, row: &TaskRow<'_>, show_repo: bool) {
    let key = row.key();
    let selected = app.backlog_view.selected_task.as_ref() == Some(&key);
    ui.horizontal(|ui| {
        if show_repo {
            status_pill(
                ui,
                crate::ui::components::StatusKind::Neutral,
                row.project.repo_name.clone(),
                None,
            );
        }
        let title = format!("{}  {}", row.task.id, row.task.title);
        if ui
            .selectable_label(selected, egui::RichText::new(title).small())
            .clicked()
        {
            app.backlog_view.selected_task = Some(key.clone());
            app.backlog_view.editor.loaded_key = None;
        }
        status_pill(
            ui,
            format::status_kind(&row.task.status),
            &row.task.status,
            None,
        );
        if row.task.is_done() {
            ui.label(egui::RichText::new("done").small().color(theme::GREEN));
        }
    });
}
