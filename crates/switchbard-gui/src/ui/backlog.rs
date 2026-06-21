//! Backlog project-management view.
//!
//! The view renders cached `backlog/` task snapshots for every tracked
//! worktree that has a Backlog project. Mutations go back through the
//! `backlog` CLI from worker threads; this module only queues intents.

use crate::app::HiveApp;
use crate::runtime::worktree_names::worktree_display_name;
use crate::runtime::{BacklogEditorState, BacklogTaskSortDirection, BacklogTaskSortKey};
use crate::ui::components::{status_pill, StatusKind};
use crate::ui::theme;
use eframe::egui;
use std::cmp::Ordering;
use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use switchbard_core::{
    BacklogProject, BacklogTask, BacklogTaskPatch, BacklogTaskSource, NewBacklogTask, Repo,
    BACKLOG_PRIORITIES, BACKLOG_STATUSES,
};

#[derive(Default)]
struct Pending {
    save: Option<(PathBuf, String, BacklogTaskPatch)>,
    bulk_save: Option<(PathBuf, Vec<String>, BacklogTaskPatch, String)>,
    toggle_ac: Option<(PathBuf, String, usize, bool)>,
    append_note: Option<(PathBuf, String, String)>,
    create: Option<(PathBuf, NewBacklogTask)>,
}

struct Snapshot {
    projects: Vec<ProjectRow>,
    filter_lc: String,
}

struct ProjectRow {
    key: PathBuf,
    project: BacklogProject,
    repo_name: String,
    worktree_label: String,
    branch: Option<String>,
}

impl ProjectRow {
    fn label(&self) -> String {
        let mut label = format!("{} / {}", self.repo_name, self.worktree_label);
        if let Some(branch) = &self.branch {
            label.push_str(&format!(" · {branch}"));
        }
        label
    }

    fn matches_filter(&self, filter_lc: &str) -> bool {
        if filter_lc.is_empty() {
            return true;
        }
        let haystack = format!(
            "{} {} {} {}",
            self.repo_name,
            self.worktree_label,
            self.branch.as_deref().unwrap_or_default(),
            self.key.display()
        )
        .to_lowercase();
        haystack.contains(filter_lc)
    }
}

impl Snapshot {
    fn collect(app: &HiveApp) -> Self {
        let projects = app.backlog_projects_snapshot();
        let repos = app.repos_snapshot();
        let worktrees = app.worktrees_snapshot();
        let repos_by_name: HashMap<String, Repo> = repos
            .iter()
            .cloned()
            .map(|repo| (repo.name.clone(), repo))
            .collect();

        let mut rows = projects
            .into_iter()
            .map(|(path, project)| {
                let worktree = worktrees.iter().find(|wt| wt.path == path);
                let repo_name = worktree
                    .map(|wt| wt.repo_name.clone())
                    .or_else(|| {
                        repos
                            .iter()
                            .find(|repo| repo.path == path)
                            .map(|repo| repo.name.clone())
                    })
                    .unwrap_or_else(|| {
                        path.file_name()
                            .and_then(|name| name.to_str())
                            .unwrap_or("project")
                            .to_string()
                    });
                let worktree_label = match (worktree, repos_by_name.get(&repo_name)) {
                    (Some(wt), Some(repo)) => worktree_display_name(&app.config, repo, wt),
                    _ => path
                        .file_name()
                        .and_then(|name| name.to_str())
                        .unwrap_or("worktree")
                        .to_string(),
                };
                ProjectRow {
                    key: path,
                    project,
                    repo_name,
                    worktree_label,
                    branch: worktree.and_then(|wt| wt.branch.clone()),
                }
            })
            .collect::<Vec<_>>();
        rows.sort_by(|a, b| {
            a.repo_name
                .cmp(&b.repo_name)
                .then_with(|| a.worktree_label.cmp(&b.worktree_label))
                .then_with(|| a.key.cmp(&b.key))
        });

        Self {
            projects: rows,
            filter_lc: app.filter.to_lowercase(),
        }
    }

    fn selected_project(&self, selected: &Option<PathBuf>) -> Option<&ProjectRow> {
        selected
            .as_ref()
            .and_then(|path| self.projects.iter().find(|row| &row.key == path))
    }
}

pub fn render(app: &mut HiveApp, ctx: &egui::Context) {
    let snap = Snapshot::collect(app);
    ensure_selection(app, &snap);
    let mut pending = Pending::default();

    egui::CentralPanel::default().show(ctx, |ui| {
        if snap.projects.is_empty() {
            render_empty(ui);
            return;
        }
        render_summary(app, ui, &snap);
        ui.add_space(6.0);
        render_project_toolbar(app, ui, &snap);
        ui.separator();
        render_task_workspace(app, ui, &snap, &mut pending);
    });

    render_create_modal(app, ctx, &snap, &mut pending);
    apply_pending(app, ctx, pending);
}

fn ensure_selection(app: &mut HiveApp, snap: &Snapshot) {
    if snap.projects.is_empty() {
        app.backlog_view.selected_project = None;
        app.backlog_view.selected_task_id = None;
        app.backlog_view.bulk_selected_task_ids.clear();
        app.backlog_view.bulk_selection_anchor_task_id = None;
        app.backlog_view.editor.loaded_key = None;
        return;
    }

    let selected_project_exists = app
        .backlog_view
        .selected_project
        .as_ref()
        .is_some_and(|path| snap.projects.iter().any(|row| &row.key == path));
    if !selected_project_exists {
        app.backlog_view.selected_project = Some(snap.projects[0].key.clone());
        app.backlog_view.selected_task_id = None;
        app.backlog_view.bulk_selected_task_ids.clear();
        app.backlog_view.bulk_selection_anchor_task_id = None;
        app.backlog_view.editor.loaded_key = None;
    }

    let Some(project) = snap.selected_project(&app.backlog_view.selected_project) else {
        return;
    };
    let current = app.backlog_view.selected_task_id.clone();
    let current_visible = current.as_ref().is_some_and(|id| {
        project
            .project
            .tasks
            .iter()
            .any(|task| &task.id == id && task_visible(task, app, &snap.filter_lc))
    });
    if !current_visible {
        app.backlog_view.selected_task_id =
            sorted_visible_tasks(&project.project, app, &snap.filter_lc)
                .first()
                .map(|task| task.id.clone());
        app.backlog_view.editor.loaded_key = None;
    }
}

fn render_empty(ui: &mut egui::Ui) {
    ui.vertical_centered(|ui| {
        ui.add_space(80.0);
        ui.heading("Backlog");
        ui.add_space(8.0);
        ui.label(
            egui::RichText::new(
                "No tracked worktrees have a backlog/config.yml or backlog/tasks directory.",
            )
            .color(theme::MUTED_TEXT),
        );
    });
}

fn render_summary(app: &mut HiveApp, ui: &mut egui::Ui, snap: &Snapshot) {
    let selected = snap.selected_project(&app.backlog_view.selected_project);
    let task_count = selected.map(|row| row.project.tasks.len()).unwrap_or(0);
    let open_count = selected
        .map(|row| open_task_count(&row.project))
        .unwrap_or(0);
    let warning_count = selected.map(|row| row.project.warnings.len()).unwrap_or(0);

    ui.horizontal(|ui| {
        ui.heading("Backlog");
        ui.separator();
        ui.label(
            egui::RichText::new(format!("{} open · {} total", open_count, task_count))
                .color(theme::WEAK_TEXT),
        );
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
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .button("Refresh Backlog")
                .on_hover_text("Reload Backlog tasks from tracked worktrees")
                .clicked()
            {
                app.backlog_kick.notify();
                app.backlog_status.set("refreshing Backlog projects");
            }
            if app.backlog_view.selected_project.is_some()
                && ui
                    .button("+ Task")
                    .on_hover_text("Create a task in the selected Backlog project")
                    .clicked()
            {
                app.backlog_view.new_task.open = true;
            }
        });
    });
}

fn render_project_toolbar(app: &mut HiveApp, ui: &mut egui::Ui, snap: &Snapshot) {
    let Some(project) = snap.selected_project(&app.backlog_view.selected_project) else {
        return;
    };
    ui.horizontal_wrapped(|ui| {
        ui.label(egui::RichText::new("Project").color(theme::MUTED_TEXT));
        ui.add(
            egui::TextEdit::singleline(&mut app.backlog_view.project_filter)
                .hint_text("Filter projects")
                .desired_width(180.0),
        );
        let project_filter_lc = app.backlog_view.project_filter.to_lowercase();
        egui::ComboBox::from_id_salt("backlog_project_picker")
            .selected_text(project.label())
            .width(280.0)
            .show_ui(ui, |ui| {
                let mut shown = 0usize;
                for row in &snap.projects {
                    if !row.matches_filter(&project_filter_lc) {
                        continue;
                    }
                    shown += 1;
                    let selected = app.backlog_view.selected_project.as_ref() == Some(&row.key);
                    let label =
                        format!("{}  ·  {} open", row.label(), open_task_count(&row.project));
                    if ui.selectable_label(selected, label).clicked() {
                        app.backlog_view.selected_project = Some(row.key.clone());
                        app.backlog_view.selected_task_id = None;
                        app.backlog_view.bulk_selected_task_ids.clear();
                        app.backlog_view.bulk_selection_anchor_task_id = None;
                        app.backlog_view.editor.loaded_key = None;
                    }
                }
                if shown == 0 {
                    ui.label(egui::RichText::new("No matching projects").color(theme::MUTED_TEXT));
                }
            });

        ui.separator();
        ui.label(egui::RichText::new("Status").color(theme::MUTED_TEXT));
        let statuses = status_options(&project.project);
        egui::ComboBox::from_id_salt("backlog_status_filter")
            .selected_text(status_filter_label(&app.backlog_view.status_filter))
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

        ui.label(egui::RichText::new("Priority").color(theme::MUTED_TEXT));
        egui::ComboBox::from_id_salt("backlog_priority_filter")
            .selected_text(priority_filter_label(&app.backlog_view.priority_filter))
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
                        priority_title(priority),
                    );
                }
            });

        ui.checkbox(&mut app.backlog_view.show_completed, "Done");
        ui.checkbox(&mut app.backlog_view.show_archived, "Archived");
        ui.separator();
        let visible = project
            .project
            .tasks
            .iter()
            .filter(|task| task_visible(task, app, &snap.filter_lc))
            .count();
        ui.label(egui::RichText::new(format!("{visible} visible")).color(theme::MUTED_TEXT));
    });
}

fn render_task_workspace(
    app: &mut HiveApp,
    ui: &mut egui::Ui,
    snap: &Snapshot,
    pending: &mut Pending,
) {
    let Some(project) = snap.selected_project(&app.backlog_view.selected_project) else {
        return;
    };
    let list_width = (ui.available_width() * 0.44).clamp(420.0, 620.0);
    let height = ui.available_height();
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.set_width(list_width);
            ui.set_min_height(height);
            render_task_list(app, ui, project, snap, pending);
        });
        ui.separator();
        ui.vertical(|ui| {
            ui.set_min_height(height);
            render_task_detail(app, ui, project, pending);
        });
    });
}

fn render_task_list(
    app: &mut HiveApp,
    ui: &mut egui::Ui,
    project: &ProjectRow,
    snap: &Snapshot,
    pending: &mut Pending,
) {
    render_task_sort_controls(app, ui);
    ui.add_space(4.0);
    let tasks = sorted_visible_tasks(&project.project, app, &snap.filter_lc);
    retain_visible_bulk_selection(app, &tasks);
    let visible_task_ids = tasks.iter().map(|task| task.id.clone()).collect::<Vec<_>>();
    ui.horizontal(|ui| {
        render_select_all_checkbox(app, ui, &tasks);
        ui.add_sized(
            [(ui.available_width() - 236.0).max(140.0), 18.0],
            egui::Label::new(egui::RichText::new("Task").small().color(theme::MUTED_TEXT)),
        );
        ui.add_sized(
            [86.0, 18.0],
            egui::Label::new(
                egui::RichText::new("Status")
                    .small()
                    .color(theme::MUTED_TEXT),
            ),
        );
        ui.add_sized(
            [62.0, 18.0],
            egui::Label::new(
                egui::RichText::new("Priority")
                    .small()
                    .color(theme::MUTED_TEXT),
            ),
        );
        ui.add_sized(
            [52.0, 18.0],
            egui::Label::new(egui::RichText::new("AC").small().color(theme::MUTED_TEXT)),
        );
    });
    ui.separator();
    egui::ScrollArea::vertical()
        .id_salt("backlog_task_list")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            let rendered = tasks.len();
            for task in tasks {
                render_task_list_row(app, ui, project, task, &visible_task_ids, pending);
                ui.add_space(5.0);
            }
            if rendered == 0 {
                ui.add_space(20.0);
                ui.label(egui::RichText::new("No tasks match the current filters").strong());
                ui.label(
                    egui::RichText::new("Adjust the filter, status, or priority.")
                        .color(theme::MUTED_TEXT),
                );
            }
        });
}

fn render_select_all_checkbox(app: &mut HiveApp, ui: &mut egui::Ui, tasks: &[&BacklogTask]) {
    let all_selected = !tasks.is_empty()
        && tasks
            .iter()
            .all(|task| app.backlog_view.bulk_selected_task_ids.contains(&task.id));
    let mut checked = all_selected;
    let response = ui
        .add_sized([24.0, 18.0], egui::Checkbox::without_text(&mut checked))
        .on_hover_text("Select all visible tasks");
    if response.clicked() {
        if all_selected {
            for task in tasks {
                app.backlog_view.bulk_selected_task_ids.remove(&task.id);
            }
            app.backlog_view.bulk_selection_anchor_task_id = None;
        } else {
            for task in tasks {
                app.backlog_view
                    .bulk_selected_task_ids
                    .insert(task.id.clone());
            }
            app.backlog_view.bulk_selection_anchor_task_id =
                tasks.first().map(|task| task.id.clone());
        }
    }
}

fn render_task_sort_controls(app: &mut HiveApp, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("Sort").color(theme::MUTED_TEXT));
        egui::ComboBox::from_id_salt("backlog_task_sort_key")
            .selected_text(app.backlog_view.sort_key.label())
            .width(118.0)
            .show_ui(ui, |ui| {
                for key in [
                    BacklogTaskSortKey::Task,
                    BacklogTaskSortKey::Status,
                    BacklogTaskSortKey::Priority,
                    BacklogTaskSortKey::AcceptanceCriteria,
                ] {
                    ui.selectable_value(&mut app.backlog_view.sort_key, key, key.label());
                }
            });
        if ui
            .button(app.backlog_view.sort_direction.label())
            .on_hover_text("Toggle task list sort direction")
            .clicked()
        {
            app.backlog_view.sort_direction = app.backlog_view.sort_direction.toggled();
        }
        let selected_count = app.backlog_view.bulk_selected_task_ids.len();
        if selected_count > 0 {
            ui.separator();
            ui.label(
                egui::RichText::new(format!("{selected_count} selected")).color(theme::WEAK_TEXT),
            );
            if ui
                .small_button("Clear")
                .on_hover_text("Clear selected tasks")
                .clicked()
            {
                app.backlog_view.bulk_selected_task_ids.clear();
                app.backlog_view.bulk_selection_anchor_task_id = None;
            }
        }
    });
}

fn render_task_list_row(
    app: &mut HiveApp,
    ui: &mut egui::Ui,
    project: &ProjectRow,
    task: &BacklogTask,
    visible_task_ids: &[String],
    pending: &mut Pending,
) {
    let detail_selected = app.backlog_view.selected_task_id.as_deref() == Some(task.id.as_str());
    let bulk_selected = app.backlog_view.bulk_selected_task_ids.contains(&task.id);
    let selected = detail_selected || bulk_selected;
    let title_width = (ui.available_width() - 236.0).max(140.0);
    let row_response = ui.horizontal(|ui| {
        let mut checked = bulk_selected;
        let checkbox = ui
            .add_sized([24.0, 26.0], egui::Checkbox::without_text(&mut checked))
            .on_hover_text("Select task for bulk actions");
        if checkbox.changed() {
            let shift = ui.input(|input| input.modifiers.shift);
            if shift {
                select_bulk_task_range(app, visible_task_ids, task);
            } else {
                set_bulk_task_selected(app, task, checked);
            }
        }
        let title = egui::RichText::new(format!("{}  {}", task.id, task.title)).strong();
        let resp = ui
            .add_sized(
                [title_width, 26.0],
                egui::Button::new(title)
                    .selected(selected)
                    .frame(false)
                    .truncate(),
            )
            .on_hover_text(task.description.as_str());
        if resp.clicked() {
            let (shift, toggle_bulk) = ui.input(|input| {
                (
                    input.modifiers.shift,
                    input.modifiers.command || input.modifiers.ctrl,
                )
            });
            if shift {
                select_bulk_task_range(app, visible_task_ids, task);
            } else if toggle_bulk {
                toggle_bulk_task_selection(app, task);
            } else {
                app.backlog_view.selected_task_id = Some(task.id.clone());
                app.backlog_view.bulk_selection_anchor_task_id = Some(task.id.clone());
                app.backlog_view.editor.loaded_key = None;
            }
        }
        ui.allocate_ui_with_layout(
            egui::vec2(86.0, 26.0),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                status_pill(ui, status_kind(&task.status), &task.status, None);
            },
        );
        ui.allocate_ui_with_layout(
            egui::vec2(62.0, 26.0),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                ui.label(
                    egui::RichText::new(priority_title(&task.priority))
                        .small()
                        .color(priority_color(&task.priority)),
                );
            },
        );
        ui.allocate_ui_with_layout(
            egui::vec2(52.0, 26.0),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                if !task.acceptance_criteria.is_empty() {
                    ui.label(
                        egui::RichText::new(format!(
                            "{}/{}",
                            task.acceptance_done_count(),
                            task.acceptance_criteria.len()
                        ))
                        .small()
                        .color(theme::MUTED_TEXT),
                    );
                } else {
                    ui.label(egui::RichText::new("-").small().color(theme::MUTED_TEXT));
                }
            },
        );
        if task.source != BacklogTaskSource::Active {
            ui.label(
                egui::RichText::new(task.source.label())
                    .small()
                    .color(theme::MUTED_TEXT),
            );
        }
    });
    if row_response.response.secondary_clicked() {
        focus_context_selection(app, task);
    }
    row_response.response.context_menu(|ui| {
        render_task_context_menu(app, ui, &project.key, &project.project, task, pending);
    });
    ui.separator();
}

fn render_task_context_menu(
    app: &mut HiveApp,
    ui: &mut egui::Ui,
    project_root: &Path,
    project: &BacklogProject,
    clicked_task: &BacklogTask,
    pending: &mut Pending,
) {
    let selected_ids = selected_task_ids_for_menu(app, project, clicked_task);
    let editable_ids = editable_task_ids(project, &selected_ids);
    ui.label(format!(
        "{} selected · {} editable",
        selected_ids.len(),
        editable_ids.len()
    ));
    if editable_ids.len() < selected_ids.len() {
        ui.label(
            egui::RichText::new("Completed, archived, and draft tasks are skipped")
                .small()
                .color(theme::MUTED_TEXT),
        );
    }
    ui.separator();

    ui.label(egui::RichText::new("Move").small().color(theme::MUTED_TEXT));
    for status in BACKLOG_STATUSES {
        let label = if status.eq_ignore_ascii_case("done") {
            "Mark Done".to_string()
        } else {
            format!("Move to {status}")
        };
        bulk_patch_button(
            app,
            ui,
            pending,
            project_root,
            &editable_ids,
            label,
            BacklogTaskPatch {
                status: Some((*status).to_string()),
                ..Default::default()
            },
        );
    }

    ui.separator();
    ui.label(
        egui::RichText::new("Priority")
            .small()
            .color(theme::MUTED_TEXT),
    );
    for priority in BACKLOG_PRIORITIES {
        bulk_patch_button(
            app,
            ui,
            pending,
            project_root,
            &editable_ids,
            format!("Set priority {}", priority_title(priority)),
            BacklogTaskPatch {
                priority: Some((*priority).to_string()),
                ..Default::default()
            },
        );
    }

    ui.separator();
    if ui.button("Clear selection").clicked() {
        app.backlog_view.bulk_selected_task_ids.clear();
        app.backlog_view.bulk_selection_anchor_task_id = None;
        ui.close_menu();
    }
}

fn bulk_patch_button(
    app: &mut HiveApp,
    ui: &mut egui::Ui,
    pending: &mut Pending,
    project_root: &Path,
    editable_ids: &[String],
    label: String,
    patch: BacklogTaskPatch,
) {
    if ui
        .add_enabled(!editable_ids.is_empty(), egui::Button::new(&label))
        .clicked()
    {
        pending.bulk_save = Some((
            project_root.to_path_buf(),
            editable_ids.to_vec(),
            patch,
            label.clone(),
        ));
        app.backlog_status
            .set(format!("{label}: updating {} task(s)", editable_ids.len()));
        ui.close_menu();
    }
}

fn selected_task_ids_for_menu(
    app: &HiveApp,
    project: &BacklogProject,
    clicked_task: &BacklogTask,
) -> Vec<String> {
    if app
        .backlog_view
        .bulk_selected_task_ids
        .contains(&clicked_task.id)
    {
        project
            .tasks
            .iter()
            .filter(|task| app.backlog_view.bulk_selected_task_ids.contains(&task.id))
            .map(|task| task.id.clone())
            .collect()
    } else {
        vec![clicked_task.id.clone()]
    }
}

fn editable_task_ids(project: &BacklogProject, selected_ids: &[String]) -> Vec<String> {
    selected_ids
        .iter()
        .filter(|id| {
            project
                .tasks
                .iter()
                .any(|task| task.id == id.as_str() && task.editable())
        })
        .cloned()
        .collect()
}

fn retain_visible_bulk_selection(app: &mut HiveApp, tasks: &[&BacklogTask]) {
    app.backlog_view
        .bulk_selected_task_ids
        .retain(|id| tasks.iter().any(|task| task.id == *id));
    if app
        .backlog_view
        .bulk_selection_anchor_task_id
        .as_ref()
        .is_some_and(|anchor| !tasks.iter().any(|task| task.id == *anchor))
    {
        app.backlog_view.bulk_selection_anchor_task_id = None;
    }
}

fn set_bulk_task_selected(app: &mut HiveApp, task: &BacklogTask, selected: bool) {
    if selected {
        app.backlog_view
            .bulk_selected_task_ids
            .insert(task.id.clone());
        app.backlog_view.bulk_selection_anchor_task_id = Some(task.id.clone());
    } else {
        app.backlog_view.bulk_selected_task_ids.remove(&task.id);
        if app.backlog_view.bulk_selection_anchor_task_id.as_deref() == Some(task.id.as_str()) {
            app.backlog_view.bulk_selection_anchor_task_id = app
                .backlog_view
                .bulk_selected_task_ids
                .iter()
                .next()
                .cloned();
        }
    }
}

fn select_bulk_task_range(app: &mut HiveApp, visible_task_ids: &[String], task: &BacklogTask) {
    let range_ids = bulk_range_ids(
        visible_task_ids,
        app.backlog_view.bulk_selection_anchor_task_id.as_deref(),
        &task.id,
    );
    if range_ids.is_empty() {
        set_bulk_task_selected(app, task, true);
        return;
    }
    for id in range_ids {
        app.backlog_view.bulk_selected_task_ids.insert(id);
    }
    app.backlog_view.bulk_selection_anchor_task_id = Some(task.id.clone());
}

fn bulk_range_ids(
    visible_task_ids: &[String],
    anchor_task_id: Option<&str>,
    clicked_task_id: &str,
) -> Vec<String> {
    let Some(clicked_index) = visible_task_ids.iter().position(|id| id == clicked_task_id) else {
        return Vec::new();
    };
    let anchor_index = anchor_task_id
        .and_then(|anchor| visible_task_ids.iter().position(|id| id == anchor))
        .unwrap_or(clicked_index);
    let (start, end) = if anchor_index <= clicked_index {
        (anchor_index, clicked_index)
    } else {
        (clicked_index, anchor_index)
    };
    visible_task_ids[start..=end].to_vec()
}

fn toggle_bulk_task_selection(app: &mut HiveApp, task: &BacklogTask) {
    if !app.backlog_view.bulk_selected_task_ids.remove(&task.id) {
        app.backlog_view
            .bulk_selected_task_ids
            .insert(task.id.clone());
    }
    app.backlog_view.bulk_selection_anchor_task_id = Some(task.id.clone());
}

fn focus_context_selection(app: &mut HiveApp, task: &BacklogTask) {
    if !app.backlog_view.bulk_selected_task_ids.contains(&task.id) {
        app.backlog_view.bulk_selected_task_ids.clear();
        app.backlog_view
            .bulk_selected_task_ids
            .insert(task.id.clone());
    }
    app.backlog_view.bulk_selection_anchor_task_id = Some(task.id.clone());
    app.backlog_view.selected_task_id = Some(task.id.clone());
    app.backlog_view.editor.loaded_key = None;
}

fn render_task_detail(
    app: &mut HiveApp,
    ui: &mut egui::Ui,
    project: &ProjectRow,
    pending: &mut Pending,
) {
    let selected_id = app.backlog_view.selected_task_id.clone();
    let Some(task) = selected_id
        .as_ref()
        .and_then(|id| project.project.tasks.iter().find(|task| &task.id == id))
    else {
        ui.vertical_centered(|ui| {
            ui.add_space(40.0);
            ui.label(egui::RichText::new("Select a task").strong());
        });
        return;
    };

    sync_editor(app, &project.key, task);
    let editable = task.editable() && project.project.cli_available();

    egui::ScrollArea::vertical()
        .id_salt("backlog_task_detail")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            render_detail_header(ui, project, task, editable);
            ui.add_space(8.0);
            render_editor(app, ui, &project.key, task, editable, pending);
            ui.add_space(10.0);
            render_acceptance(app, ui, &project.key, task, editable, pending);
            ui.add_space(10.0);
            render_notes(app, ui, &project.key, task, editable, pending);
            render_readonly_sections(ui, task);
        });
}

fn render_detail_header(
    ui: &mut egui::Ui,
    project: &ProjectRow,
    task: &BacklogTask,
    editable: bool,
) {
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(&task.id)
                .monospace()
                .color(theme::MUTED_TEXT),
        );
        status_pill(ui, status_kind(&task.status), &task.status, None);
        ui.label(
            egui::RichText::new(priority_title(&task.priority))
                .color(priority_color(&task.priority)),
        );
        if !editable {
            status_pill(
                ui,
                StatusKind::Neutral,
                "read-only",
                Some("Only active backlog/tasks entries are edited through the CLI"),
            );
        }
    });
    ui.heading(&task.title);
    ui.label(
        egui::RichText::new(format!(
            "{} / {}",
            project.repo_name, project.worktree_label
        ))
        .small()
        .color(theme::MUTED_TEXT),
    )
    .on_hover_text(task.path.display().to_string());
    if !project.project.warnings.is_empty() {
        for warning in &project.project.warnings {
            ui.colored_label(theme::AMBER, warning);
        }
    }
}

fn render_editor(
    app: &mut HiveApp,
    ui: &mut egui::Ui,
    project_root: &Path,
    task: &BacklogTask,
    editable: bool,
    pending: &mut Pending,
) {
    ui.label(egui::RichText::new("Task").strong());
    let mut status_save: Option<String> = None;
    ui.add_enabled_ui(editable, |ui| {
        ui.label("title");
        ui.add(
            egui::TextEdit::singleline(&mut app.backlog_view.editor.title)
                .desired_width(f32::INFINITY),
        );

        ui.horizontal(|ui| {
            ui.label("status");
            if render_value_combo(
                ui,
                "backlog_task_status",
                &mut app.backlog_view.editor.status,
                BACKLOG_STATUSES,
                title_case_value,
            ) {
                status_save = Some(app.backlog_view.editor.status.trim().to_string());
            }
            ui.label("priority");
            render_value_combo(
                ui,
                "backlog_task_priority",
                &mut app.backlog_view.editor.priority,
                BACKLOG_PRIORITIES,
                priority_title,
            );
        });

        ui.horizontal(|ui| {
            ui.label("labels");
            ui.add(
                egui::TextEdit::singleline(&mut app.backlog_view.editor.labels)
                    .desired_width(260.0),
            );
            ui.label("assignees");
            ui.add(
                egui::TextEdit::singleline(&mut app.backlog_view.editor.assignees)
                    .desired_width(180.0),
            );
        });

        ui.label("description");
        ui.add(
            egui::TextEdit::multiline(&mut app.backlog_view.editor.description)
                .desired_rows(5)
                .desired_width(f32::INFINITY),
        );
    });

    let mut patch = patch_from_editor(task, &app.backlog_view.editor);
    if let Some(new_status) =
        status_save.filter(|status| !status.eq_ignore_ascii_case(task.status.trim()))
    {
        pending.save = Some((
            project_root.to_path_buf(),
            task.id.clone(),
            BacklogTaskPatch {
                status: Some(new_status),
                ..Default::default()
            },
        ));
        patch.status = None;
        app.backlog_status
            .set(format!("updating {} status", task.id));
    }
    let can_save = editable && !patch.is_empty();
    ui.horizontal(|ui| {
        if ui
            .add_enabled(can_save, egui::Button::new("Save"))
            .on_hover_text("Save task fields through backlog task edit")
            .clicked()
        {
            pending.save = Some((project_root.to_path_buf(), task.id.clone(), patch));
        }
        if !editable {
            ui.label(
                egui::RichText::new("Backlog CLI edits are enabled for active tasks only.")
                    .color(theme::MUTED_TEXT),
            );
        }
    });
}

fn render_acceptance(
    app: &mut HiveApp,
    ui: &mut egui::Ui,
    project_root: &Path,
    task: &BacklogTask,
    editable: bool,
    pending: &mut Pending,
) {
    ui.label(egui::RichText::new("Acceptance Criteria").strong());
    if task.acceptance_criteria.is_empty() {
        ui.label(egui::RichText::new("No acceptance criteria").color(theme::MUTED_TEXT));
        return;
    }
    for item in &task.acceptance_criteria {
        let mut checked = item.checked;
        let response = ui
            .add_enabled_ui(editable, |ui| {
                ui.checkbox(&mut checked, format!("#{} {}", item.index, item.text))
            })
            .inner;
        if response.changed() {
            pending.toggle_ac = Some((
                project_root.to_path_buf(),
                task.id.clone(),
                item.index,
                checked,
            ));
            app.backlog_status
                .set(format!("updating {} AC #{}", task.id, item.index));
        }
    }
}

fn render_notes(
    app: &mut HiveApp,
    ui: &mut egui::Ui,
    project_root: &Path,
    task: &BacklogTask,
    editable: bool,
    pending: &mut Pending,
) {
    ui.label(egui::RichText::new("Implementation Notes").strong());
    if task.implementation_notes.trim().is_empty() {
        ui.label(egui::RichText::new("No notes yet").color(theme::MUTED_TEXT));
    } else {
        egui::ScrollArea::vertical()
            .id_salt(format!("notes_{}", task.id))
            .max_height(140.0)
            .show(ui, |ui| {
                ui.label(&task.implementation_notes);
            });
    }
    ui.add_space(4.0);
    ui.add_enabled_ui(editable, |ui| {
        ui.add(
            egui::TextEdit::multiline(&mut app.backlog_view.editor.note)
                .hint_text("Append note")
                .desired_rows(3)
                .desired_width(f32::INFINITY),
        );
        let can_append = !app.backlog_view.editor.note.trim().is_empty();
        if ui
            .add_enabled(can_append, egui::Button::new("Append Note"))
            .clicked()
        {
            pending.append_note = Some((
                project_root.to_path_buf(),
                task.id.clone(),
                app.backlog_view.editor.note.trim().to_string(),
            ));
            app.backlog_view.editor.note.clear();
        }
    });
}

fn render_readonly_sections(ui: &mut egui::Ui, task: &BacklogTask) {
    if !task.implementation_plan.trim().is_empty() {
        egui::CollapsingHeader::new("Implementation Plan")
            .default_open(false)
            .show(ui, |ui| {
                ui.label(&task.implementation_plan);
            });
    }
    if !task.definition_of_done.is_empty() {
        egui::CollapsingHeader::new("Definition of Done")
            .default_open(false)
            .show(ui, |ui| {
                for item in &task.definition_of_done {
                    let mark = if item.checked { "[x]" } else { "[ ]" };
                    ui.label(format!("{mark} #{} {}", item.index, item.text));
                }
            });
    }
    if !task.final_summary.trim().is_empty() {
        egui::CollapsingHeader::new("Final Summary")
            .default_open(false)
            .show(ui, |ui| {
                ui.label(&task.final_summary);
            });
    }
}

fn render_create_modal(
    app: &mut HiveApp,
    ctx: &egui::Context,
    snap: &Snapshot,
    pending: &mut Pending,
) {
    if !app.backlog_view.new_task.open {
        return;
    }
    let selected_project = app.backlog_view.selected_project.clone();
    let Some(project) = snap.selected_project(&selected_project) else {
        app.backlog_view.new_task.open = false;
        return;
    };

    let mut open = true;
    let mut close = false;
    egui::Window::new("New Backlog Task")
        .open(&mut open)
        .collapsible(false)
        .resizable(true)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            ui.label(
                egui::RichText::new(format!(
                    "{} / {}",
                    project.repo_name, project.worktree_label
                ))
                .color(theme::MUTED_TEXT),
            );
            ui.label("title");
            ui.add(
                egui::TextEdit::singleline(&mut app.backlog_view.new_task.title)
                    .desired_width(520.0),
            );
            ui.label("description");
            ui.add(
                egui::TextEdit::multiline(&mut app.backlog_view.new_task.description)
                    .desired_rows(4)
                    .desired_width(520.0),
            );
            ui.horizontal(|ui| {
                ui.label("status");
                render_value_combo(
                    ui,
                    "backlog_new_status",
                    &mut app.backlog_view.new_task.status,
                    BACKLOG_STATUSES,
                    title_case_value,
                );
                ui.label("priority");
                render_value_combo(
                    ui,
                    "backlog_new_priority",
                    &mut app.backlog_view.new_task.priority,
                    BACKLOG_PRIORITIES,
                    priority_title,
                );
            });
            ui.label("acceptance criteria");
            ui.add(
                egui::TextEdit::multiline(&mut app.backlog_view.new_task.acceptance_criteria)
                    .hint_text("One criterion per line")
                    .desired_rows(4)
                    .desired_width(520.0),
            );
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                let can_create = project.project.cli_available()
                    && !app.backlog_view.new_task.title.trim().is_empty();
                if ui
                    .add_enabled(can_create, egui::Button::new("Create"))
                    .clicked()
                {
                    let criteria = app
                        .backlog_view
                        .new_task
                        .acceptance_criteria
                        .lines()
                        .map(str::trim)
                        .filter(|line| !line.is_empty())
                        .map(str::to_string)
                        .collect();
                    pending.create = Some((
                        project.key.clone(),
                        NewBacklogTask {
                            title: app.backlog_view.new_task.title.trim().to_string(),
                            description: app.backlog_view.new_task.description.trim().to_string(),
                            status: app.backlog_view.new_task.status.clone(),
                            priority: app.backlog_view.new_task.priority.clone(),
                            acceptance_criteria: criteria,
                        },
                    ));
                    app.backlog_view.new_task = Default::default();
                    close = true;
                }
                if ui.button("Cancel").clicked() {
                    close = true;
                }
                if !project.project.cli_available() {
                    ui.label(
                        egui::RichText::new("Backlog CLI is not available").color(theme::AMBER),
                    );
                }
            });
        });
    app.backlog_view.new_task.open = open && !close;
}

fn apply_pending(app: &mut HiveApp, ctx: &egui::Context, pending: Pending) {
    if let Some((project_root, task_id, patch)) = pending.save {
        app.spawn_backlog_save(project_root, task_id, patch, ctx);
    }
    if let Some((project_root, task_ids, patch, action_label)) = pending.bulk_save {
        app.spawn_backlog_bulk_save(project_root, task_ids, patch, action_label, ctx);
    }
    if let Some((project_root, task_id, index, checked)) = pending.toggle_ac {
        app.spawn_backlog_acceptance_toggle(project_root, task_id, index, checked, ctx);
    }
    if let Some((project_root, task_id, note)) = pending.append_note {
        app.spawn_backlog_append_note(project_root, task_id, note, ctx);
    }
    if let Some((project_root, task)) = pending.create {
        app.spawn_backlog_create(project_root, task, ctx);
    }
}

fn sync_editor(app: &mut HiveApp, project_root: &Path, task: &BacklogTask) {
    let key = format!(
        "{}::{}::{}",
        project_root.display(),
        task.id,
        task.updated_date.as_deref().unwrap_or("")
    );
    if app.backlog_view.editor.loaded_key.as_deref() == Some(key.as_str()) {
        return;
    }
    app.backlog_view.editor.loaded_key = Some(key);
    app.backlog_view.editor.title = task.title.clone();
    app.backlog_view.editor.description = task.description.clone();
    app.backlog_view.editor.status = task.status.clone();
    app.backlog_view.editor.priority = task.priority.clone();
    app.backlog_view.editor.labels = task.labels.join(", ");
    app.backlog_view.editor.assignees = task.assignees.join(", ");
    app.backlog_view.editor.note.clear();
}

fn patch_from_editor(task: &BacklogTask, editor: &BacklogEditorState) -> BacklogTaskPatch {
    let mut patch = BacklogTaskPatch::default();
    let title = editor.title.trim().to_string();
    if title != task.title {
        patch.title = Some(title);
    }
    let description = editor.description.trim().to_string();
    if description != task.description {
        patch.description = Some(description);
    }
    if !editor
        .status
        .trim()
        .eq_ignore_ascii_case(task.status.trim())
    {
        patch.status = Some(editor.status.trim().to_string());
    }
    if !editor
        .priority
        .trim()
        .eq_ignore_ascii_case(task.priority.trim())
    {
        patch.priority = Some(editor.priority.trim().to_string());
    }
    let labels = split_csv(&editor.labels);
    if labels != task.labels {
        patch.labels = Some(labels);
    }
    let assignees = split_csv(&editor.assignees);
    if assignees != task.assignees {
        patch.assignees = Some(assignees);
    }
    patch
}

fn split_csv(text: &str) -> Vec<String> {
    text.split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(str::to_string)
        .collect()
}

fn sorted_visible_tasks<'a>(
    project: &'a BacklogProject,
    app: &HiveApp,
    filter_lc: &str,
) -> Vec<&'a BacklogTask> {
    let mut tasks = project
        .tasks
        .iter()
        .filter(|task| task_visible(task, app, filter_lc))
        .collect::<Vec<_>>();
    let sort_key = app.backlog_view.sort_key;
    let sort_direction = app.backlog_view.sort_direction;
    tasks.sort_by(|a, b| compare_tasks(a, b, sort_key, sort_direction));
    tasks
}

fn compare_tasks(
    a: &BacklogTask,
    b: &BacklogTask,
    sort_key: BacklogTaskSortKey,
    sort_direction: BacklogTaskSortDirection,
) -> Ordering {
    let primary = match sort_key {
        BacklogTaskSortKey::Task => cmp_ascii_case_insensitive(&a.id, &b.id)
            .then_with(|| cmp_ascii_case_insensitive(&a.title, &b.title)),
        BacklogTaskSortKey::Status => status_rank(&a.status)
            .cmp(&status_rank(&b.status))
            .then_with(|| cmp_ascii_case_insensitive(&a.status, &b.status)),
        BacklogTaskSortKey::Priority => priority_rank(&a.priority)
            .cmp(&priority_rank(&b.priority))
            .then_with(|| cmp_ascii_case_insensitive(&a.priority, &b.priority)),
        BacklogTaskSortKey::AcceptanceCriteria => acceptance_progress(a)
            .cmp(&acceptance_progress(b))
            .then_with(|| {
                a.acceptance_criteria
                    .len()
                    .cmp(&b.acceptance_criteria.len())
            }),
    };
    let primary = match sort_direction {
        BacklogTaskSortDirection::Ascending => primary,
        BacklogTaskSortDirection::Descending => primary.reverse(),
    };
    primary
        .then_with(|| cmp_ascii_case_insensitive(&a.id, &b.id))
        .then_with(|| cmp_ascii_case_insensitive(&a.title, &b.title))
}

fn status_rank(status: &str) -> usize {
    BACKLOG_STATUSES
        .iter()
        .position(|option| option.eq_ignore_ascii_case(status))
        .unwrap_or(BACKLOG_STATUSES.len())
}

fn priority_rank(priority: &str) -> usize {
    BACKLOG_PRIORITIES
        .iter()
        .position(|option| option.eq_ignore_ascii_case(priority))
        .unwrap_or(BACKLOG_PRIORITIES.len())
}

fn acceptance_progress(task: &BacklogTask) -> usize {
    let total = task.acceptance_criteria.len();
    if total == 0 {
        return 0;
    }
    task.acceptance_done_count() * 1_000 / total
}

fn cmp_ascii_case_insensitive(a: &str, b: &str) -> Ordering {
    let mut a = a.bytes();
    let mut b = b.bytes();
    loop {
        match (a.next(), b.next()) {
            (Some(left), Some(right)) => {
                let ordering = left.to_ascii_lowercase().cmp(&right.to_ascii_lowercase());
                if ordering != Ordering::Equal {
                    return ordering;
                }
            }
            (Some(_), None) => return Ordering::Greater,
            (None, Some(_)) => return Ordering::Less,
            (None, None) => return Ordering::Equal,
        }
    }
}

fn task_visible(task: &BacklogTask, app: &HiveApp, filter_lc: &str) -> bool {
    if task_is_completed(task) && !app.backlog_view.show_completed {
        return false;
    }
    if task.source == BacklogTaskSource::Archived && !app.backlog_view.show_archived {
        return false;
    }
    if app.backlog_view.status_filter != "all"
        && !task
            .status
            .eq_ignore_ascii_case(&app.backlog_view.status_filter)
    {
        return false;
    }
    if app.backlog_view.priority_filter != "all"
        && !task
            .priority
            .eq_ignore_ascii_case(&app.backlog_view.priority_filter)
    {
        return false;
    }
    if filter_lc.is_empty() {
        return true;
    }
    let haystack = [
        task.id.as_str(),
        task.title.as_str(),
        task.status.as_str(),
        task.priority.as_str(),
        task.description.as_str(),
        &task.labels.join(" "),
        &task.assignees.join(" "),
    ]
    .join(" ")
    .to_lowercase();
    haystack.contains(filter_lc)
}

fn open_task_count(project: &BacklogProject) -> usize {
    project
        .tasks
        .iter()
        .filter(|task| !task_is_completed(task) && task.source != BacklogTaskSource::Archived)
        .count()
}

fn task_is_completed(task: &BacklogTask) -> bool {
    task.source == BacklogTaskSource::Completed || task.status.eq_ignore_ascii_case("done")
}

fn status_options(project: &BacklogProject) -> Vec<String> {
    let mut set = BTreeSet::new();
    for status in BACKLOG_STATUSES {
        set.insert((*status).to_string());
    }
    for task in &project.tasks {
        set.insert(task.status.clone());
    }
    set.into_iter().collect()
}

fn render_value_combo(
    ui: &mut egui::Ui,
    id: &'static str,
    value: &mut String,
    options: &[&str],
    label: fn(&str) -> String,
) -> bool {
    let before = value.clone();
    egui::ComboBox::from_id_salt(id)
        .selected_text(label(value))
        .show_ui(ui, |ui| {
            for option in options {
                ui.selectable_value(value, (*option).to_string(), label(option));
            }
            if !options
                .iter()
                .any(|option| option.eq_ignore_ascii_case(value))
            {
                ui.selectable_value(value, value.clone(), label(value));
            }
        });
    before != *value
}

fn status_filter_label(status: &str) -> String {
    if status == "all" {
        "All".to_string()
    } else {
        status.to_string()
    }
}

fn priority_filter_label(priority: &str) -> String {
    if priority == "all" {
        "All".to_string()
    } else {
        priority_title(priority)
    }
}

fn title_case_value(value: &str) -> String {
    value.to_string()
}

fn priority_title(priority: &str) -> String {
    match priority.to_ascii_lowercase().as_str() {
        "high" => "High".to_string(),
        "medium" => "Medium".to_string(),
        "low" => "Low".to_string(),
        _ => priority.to_string(),
    }
}

fn priority_color(priority: &str) -> egui::Color32 {
    match priority.to_ascii_lowercase().as_str() {
        "high" => theme::WARN_ORANGE,
        "medium" => theme::SKY,
        "low" => theme::MUTED_TEXT,
        _ => theme::WEAK_TEXT,
    }
}

fn status_kind(status: &str) -> StatusKind {
    match status.to_ascii_lowercase().as_str() {
        "done" => StatusKind::Good,
        "in progress" => StatusKind::Info,
        "blocked" => StatusKind::Danger,
        "to do" => StatusKind::Neutral,
        _ => StatusKind::Warn,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn done_status_counts_as_completed_even_before_cleanup_moves_file() {
        let task = task_with_status("Done", BacklogTaskSource::Active);

        assert!(task_is_completed(&task));
    }

    #[test]
    fn open_task_count_excludes_done_and_archived_tasks() {
        let project = BacklogProject {
            root: PathBuf::from("/tmp/project"),
            cli_path: None,
            tasks: vec![
                task_with_status("To Do", BacklogTaskSource::Active),
                task_with_status("In Progress", BacklogTaskSource::Active),
                task_with_status("Done", BacklogTaskSource::Active),
                task_with_status("To Do", BacklogTaskSource::Archived),
            ],
            warnings: vec![],
            loaded_at_unix: 0,
        };

        assert_eq!(open_task_count(&project), 2);
    }

    #[test]
    fn priority_sort_uses_backlog_priority_order_in_both_directions() {
        let high = task_with_fields("TASK-1", "High priority", "To Do", "high", 0, 0);
        let medium = task_with_fields("TASK-2", "Medium priority", "To Do", "medium", 0, 0);
        let low = task_with_fields("TASK-3", "Low priority", "To Do", "low", 0, 0);
        let mut tasks = [&low, &high, &medium];

        tasks.sort_by(|a, b| {
            compare_tasks(
                a,
                b,
                BacklogTaskSortKey::Priority,
                BacklogTaskSortDirection::Ascending,
            )
        });
        assert_eq!(
            tasks
                .iter()
                .map(|task| task.id.as_str())
                .collect::<Vec<_>>(),
            vec!["TASK-1", "TASK-2", "TASK-3"]
        );

        tasks.sort_by(|a, b| {
            compare_tasks(
                a,
                b,
                BacklogTaskSortKey::Priority,
                BacklogTaskSortDirection::Descending,
            )
        });
        assert_eq!(
            tasks
                .iter()
                .map(|task| task.id.as_str())
                .collect::<Vec<_>>(),
            vec!["TASK-3", "TASK-2", "TASK-1"]
        );
    }

    #[test]
    fn acceptance_sort_orders_by_completion_progress() {
        let empty = task_with_fields("TASK-1", "No AC", "To Do", "medium", 0, 0);
        let partial = task_with_fields("TASK-2", "Partial AC", "To Do", "medium", 1, 3);
        let complete = task_with_fields("TASK-3", "Complete AC", "To Do", "medium", 2, 2);
        let mut tasks = [&complete, &empty, &partial];

        tasks.sort_by(|a, b| {
            compare_tasks(
                a,
                b,
                BacklogTaskSortKey::AcceptanceCriteria,
                BacklogTaskSortDirection::Ascending,
            )
        });
        assert_eq!(
            tasks
                .iter()
                .map(|task| task.id.as_str())
                .collect::<Vec<_>>(),
            vec!["TASK-1", "TASK-2", "TASK-3"]
        );
    }

    #[test]
    fn editable_task_ids_skips_readonly_backlog_sources() {
        let active = task_with_fields("TASK-1", "Active", "To Do", "medium", 0, 0);
        let mut completed = task_with_fields("TASK-2", "Completed", "Done", "medium", 0, 0);
        completed.source = BacklogTaskSource::Completed;
        let mut archived = task_with_fields("TASK-3", "Archived", "To Do", "medium", 0, 0);
        archived.source = BacklogTaskSource::Archived;
        let project = BacklogProject {
            root: PathBuf::from("/tmp/project"),
            cli_path: None,
            tasks: vec![active, completed, archived],
            warnings: vec![],
            loaded_at_unix: 0,
        };

        let ids = vec![
            "TASK-1".to_string(),
            "TASK-2".to_string(),
            "TASK-3".to_string(),
        ];

        assert_eq!(editable_task_ids(&project, &ids), vec!["TASK-1"]);
    }

    #[test]
    fn bulk_range_ids_selects_contiguous_visible_range() {
        let ids = task_ids(["TASK-1", "TASK-2", "TASK-3", "TASK-4"]);

        assert_eq!(
            bulk_range_ids(&ids, Some("TASK-1"), "TASK-3"),
            task_ids(["TASK-1", "TASK-2", "TASK-3"])
        );
        assert_eq!(
            bulk_range_ids(&ids, Some("TASK-4"), "TASK-2"),
            task_ids(["TASK-2", "TASK-3", "TASK-4"])
        );
    }

    #[test]
    fn bulk_range_ids_falls_back_to_clicked_task_without_visible_anchor() {
        let ids = task_ids(["TASK-1", "TASK-2", "TASK-3"]);

        assert_eq!(
            bulk_range_ids(&ids, Some("TASK-99"), "TASK-2"),
            task_ids(["TASK-2"])
        );
        assert!(bulk_range_ids(&ids, Some("TASK-1"), "TASK-99").is_empty());
    }

    fn task_ids<const N: usize>(ids: [&str; N]) -> Vec<String> {
        ids.into_iter().map(str::to_string).collect()
    }

    fn task_with_status(status: &str, source: BacklogTaskSource) -> BacklogTask {
        let mut task = task_with_fields(
            &format!("TASK-{}", status.replace(' ', "-")),
            status,
            status,
            "medium",
            0,
            0,
        );
        task.source = source;
        task
    }

    fn task_with_fields(
        id: &str,
        title: &str,
        status: &str,
        priority: &str,
        checked_criteria: usize,
        total_criteria: usize,
    ) -> BacklogTask {
        BacklogTask {
            id: id.to_string(),
            title: title.to_string(),
            status: status.to_string(),
            priority: priority.to_string(),
            assignees: vec![],
            labels: vec![],
            dependencies: vec![],
            milestone: None,
            parent: None,
            created_date: None,
            updated_date: None,
            description: String::new(),
            implementation_plan: String::new(),
            implementation_notes: String::new(),
            final_summary: String::new(),
            acceptance_criteria: (0..total_criteria)
                .map(|index| switchbard_core::BacklogChecklistItem {
                    index: index + 1,
                    checked: index < checked_criteria,
                    text: format!("Criterion {}", index + 1),
                })
                .collect(),
            definition_of_done: vec![],
            source: BacklogTaskSource::Active,
            path: PathBuf::from("/tmp/project/backlog/tasks/task.md"),
        }
    }
}
