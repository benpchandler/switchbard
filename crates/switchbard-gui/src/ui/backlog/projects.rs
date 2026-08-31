//! The Projects lens: every visible task grouped by `BacklogTask::project`,
//! cross-repo, with projects nested under their initiative and an
//! "Unassigned" bucket last. Project *assignment* lives in the detail pane
//! (`detail::render_editor`); def lifecycle is CLI-first (`switchbard-task
//! project create/edit`); this lens is read/browse-only — clicking a task
//! selects it, which the persistent detail rail shows regardless of lens.
//!
//! The Initiative → Project *structure* (which names exist, referenced ∪
//! defined; first-def-wins conflicts; the `None` bucket last) comes from
//! `switchbard_core::compute_hierarchy_rollup` — the same single authority
//! the CLI's `project`/`initiative` verbs render — so the two surfaces
//! cannot drift. This lens only joins the frame's *visible* rows onto that
//! structure: the `N/M done` counts and progress bars reflect the filtered
//! view (a project whose rows are all filtered out shows 0/0), while status
//! pills and target dates come from the rollup's def data. No IO: defs ride
//! the backlog worker's snapshot. When no named initiative exists anywhere
//! in view, the initiative header level is skipped entirely — a lone
//! "No initiative" wrapper around everything would be pure noise.

use super::{format, RepoRow, Snapshot, TaskRow};
use crate::app::HiveApp;
use crate::ui::components::{status_pill, StatusKind};
use crate::ui::theme;
use eframe::egui;
use std::collections::BTreeMap;
use switchbard_core::{compute_hierarchy_rollup, InitiativeRollup, ProjectRollup};

const UNASSIGNED_LABEL: &str = "Unassigned";
const PROGRESS_BAR_WIDTH: f32 = 120.0;

/// One project's group for this frame: the core rollup entry (structure +
/// def metadata) joined to the *visible* task rows.
struct ProjectGroup<'a> {
    rollup: ProjectRollup,
    rows: Vec<&'a TaskRow<'a>>,
}

/// The frame's full grouping, in the rollup's own order (initiatives
/// name-sorted, `None` bucket last; projects name-sorted within), plus the
/// visible rows with no project at all.
struct Groups<'a> {
    initiatives: Vec<(InitiativeRollup, Vec<ProjectGroup<'a>>)>,
    unassigned: Vec<&'a TaskRow<'a>>,
}

impl Groups<'_> {
    /// Nothing to show at all — no project exists (referenced or defined)
    /// and no unassigned row is visible. Note a project *shell* still
    /// renders when filters hide all of its rows; see the module doc.
    fn is_empty(&self) -> bool {
        self.unassigned.is_empty()
            && self
                .initiatives
                .iter()
                .all(|(_, projects)| projects.is_empty())
    }

    /// Whether the initiative header level carries any information — false
    /// when the only bucket is the no-initiative one.
    fn has_named_initiative(&self) -> bool {
        self.initiatives
            .iter()
            .any(|(initiative, _)| initiative.name.is_some())
    }
}

fn initiative_groups<'a>(scoped: &[&RepoRow], tasks: &'a [TaskRow<'a>]) -> Groups<'a> {
    let repos: Vec<&switchbard_core::BacklogRepo> = scoped.iter().map(|row| &row.repo).collect();
    let rollup = compute_hierarchy_rollup(&repos);

    let mut rows_by_project: BTreeMap<&str, Vec<&TaskRow<'_>>> = BTreeMap::new();
    let mut unassigned: Vec<&TaskRow<'_>> = Vec::new();
    for row in tasks {
        match row.task.project.as_deref() {
            Some(project) => rows_by_project.entry(project).or_default().push(row),
            None => unassigned.push(row),
        }
    }

    let initiatives = rollup
        .initiatives
        .into_iter()
        .map(|initiative| {
            let projects = initiative
                .projects
                .iter()
                .map(|project| ProjectGroup {
                    rows: rows_by_project
                        .remove(project.name.as_str())
                        .unwrap_or_default(),
                    rollup: project.clone(),
                })
                .collect();
            (initiative, projects)
        })
        .collect();

    Groups {
        initiatives,
        unassigned,
    }
}

pub(super) fn render_projects(
    app: &mut HiveApp,
    ui: &mut egui::Ui,
    snap: &Snapshot,
    tasks: Vec<TaskRow<'_>>,
) {
    let show_repo = app.backlog_view.selected_repo.is_none();
    let scoped = super::scoped_repos(app, snap);
    let groups = initiative_groups(&scoped, &tasks);

    egui::ScrollArea::vertical()
        .id_salt("backlog_projects")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            if groups.is_empty() {
                ui.add_space(20.0);
                ui.label(egui::RichText::new("No tasks match the current filters").strong());
                return;
            }
            let nest_under_initiatives = groups.has_named_initiative();
            for (initiative, projects) in &groups.initiatives {
                if nest_under_initiatives {
                    render_initiative(app, ui, initiative, projects, show_repo);
                } else {
                    for project in projects {
                        render_project(app, ui, project, show_repo);
                    }
                }
            }
            if !groups.unassigned.is_empty() {
                render_unassigned(app, ui, &groups.unassigned, show_repo);
            }
        });
}

fn render_initiative(
    app: &mut HiveApp,
    ui: &mut egui::Ui,
    initiative: &InitiativeRollup,
    projects: &[ProjectGroup<'_>],
    show_repo: bool,
) {
    let label = initiative.name.as_deref().unwrap_or("No initiative");
    let (done, total) = projects.iter().fold((0usize, 0usize), |(d, t), p| {
        (
            d + p.rows.iter().filter(|r| r.task.is_done()).count(),
            t + p.rows.len(),
        )
    });
    egui::CollapsingHeader::new(
        egui::RichText::new(format!("{label}  ·  {done}/{total} done")).strong(),
    )
    .default_open(true)
    .id_salt(format!("initiative_{label}"))
    .show(ui, |ui| {
        if initiative.has_def {
            ui.horizontal(|ui| {
                def_meta(
                    ui,
                    initiative.status.as_deref(),
                    initiative.target_date.as_deref(),
                );
            });
        }
        for project in projects {
            render_project(app, ui, project, show_repo);
        }
    });
}

fn render_project(
    app: &mut HiveApp,
    ui: &mut egui::Ui,
    project: &ProjectGroup<'_>,
    show_repo: bool,
) {
    let done = project.rows.iter().filter(|r| r.task.is_done()).count();
    let total = project.rows.len();
    egui::CollapsingHeader::new(format!("{}  ·  {done}/{total} done", project.rollup.name))
        .default_open(true)
        .id_salt(format!("project_{}", project.rollup.name))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                def_meta(
                    ui,
                    project.rollup.status.as_deref(),
                    project.rollup.target_date.as_deref(),
                );
                // `max(1)` keeps a defined-but-empty project (0/0) at an
                // honest empty bar instead of NaN.
                ui.add(
                    egui::ProgressBar::new(done as f32 / total.max(1) as f32)
                        .desired_width(PROGRESS_BAR_WIDTH),
                );
            });
            for row in &project.rows {
                render_row(app, ui, row, show_repo);
            }
        });
}

/// The Unassigned bucket: plain task group, no def metadata or progress bar
/// — it's the catch-all, not a project anyone tracks progress toward.
fn render_unassigned(app: &mut HiveApp, ui: &mut egui::Ui, rows: &[&TaskRow<'_>], show_repo: bool) {
    let done = rows.iter().filter(|row| row.task.is_done()).count();
    let total = rows.len();
    egui::CollapsingHeader::new(format!("{UNASSIGNED_LABEL}  ·  {done}/{total} done"))
        .default_open(true)
        .id_salt(format!("project_{UNASSIGNED_LABEL}"))
        .show(ui, |ui| {
            for row in rows {
                render_row(app, ui, row, show_repo);
            }
        });
}

/// Definition metadata rendered inline (no layout of its own, so the two
/// call sites can compose it with their own header rows): a status pill
/// when the def declares one, and the target date when set.
fn def_meta(ui: &mut egui::Ui, status: Option<&str>, target_date: Option<&str>) {
    if let Some(status) = status {
        status_pill(ui, project_status_kind(status), status, None);
    }
    if let Some(target) = target_date {
        ui.label(
            egui::RichText::new(format!("target {target}"))
                .small()
                .color(theme::muted_text()),
        );
    }
}

/// Map the def lifecycle vocabulary onto the shared pill semantics. Unknown
/// (hand-written) statuses render as neutral rather than being coerced.
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

fn render_row(app: &mut HiveApp, ui: &mut egui::Ui, row: &TaskRow<'_>, show_repo: bool) {
    let key = row.key();
    let selected = app.backlog_view.selected_task.as_ref() == Some(&key);
    ui.horizontal(|ui| {
        if show_repo {
            status_pill(
                ui,
                crate::ui::components::StatusKind::Neutral,
                row.repo.repo_name.clone(),
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
            ui.label(egui::RichText::new("done").small().color(theme::green()));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use switchbard_core::{BacklogRepo, BacklogTask, BacklogTaskSource, InitiativeDef, ProjectDef};

    fn task(id: &str, project: Option<&str>, done: bool) -> BacklogTask {
        BacklogTask {
            id: id.to_string(),
            title: id.to_string(),
            status: if done { "Done" } else { "To Do" }.to_string(),
            priority: "medium".to_string(),
            assignees: vec![],
            labels: vec![],
            dependencies: vec![],
            references: vec![],
            project: project.map(str::to_string),
            parent: None,
            created_date: None,
            updated_date: None,
            description: String::new(),
            implementation_plan: String::new(),
            implementation_notes: String::new(),
            final_summary: String::new(),
            acceptance_criteria: vec![],
            definition_of_done: vec![],
            source: BacklogTaskSource::Active,
            path: PathBuf::from("/tmp/fixture/backlog/tasks/t.md"),
        }
    }

    fn repo_row(
        tasks: Vec<BacklogTask>,
        project_defs: Vec<ProjectDef>,
        initiative_defs: Vec<InitiativeDef>,
    ) -> RepoRow {
        RepoRow {
            key: PathBuf::from("/tmp/fixture"),
            repo_name: "fixture".to_string(),
            worktree_label: "main".to_string(),
            branch: None,
            repo: BacklogRepo {
                root: PathBuf::from("/tmp/fixture"),
                tasks,
                warnings: vec![],
                project_defs,
                initiative_defs,
                goals: vec![],
                loaded_at_unix: 0,
                configured_statuses: vec![],
            },
        }
    }

    fn def(name: &str, initiative: Option<&str>) -> ProjectDef {
        ProjectDef {
            name: name.to_string(),
            status: "Planned".to_string(),
            target_date: None,
            initiative: initiative.map(str::to_string),
            lead: None,
            description: String::new(),
            path: PathBuf::from("/tmp/fixture/backlog/projects/p.md"),
        }
    }

    fn rows(row: &RepoRow) -> Vec<TaskRow<'_>> {
        row.repo
            .tasks
            .iter()
            .map(|task| TaskRow { repo: row, task })
            .collect()
    }

    #[test]
    fn without_any_named_initiative_the_header_level_is_skipped() {
        let row = repo_row(
            vec![task("TASK-1", Some("Alpha"), false)],
            vec![def("Alpha", None)],
            vec![],
        );
        let tasks = rows(&row);
        let groups = initiative_groups(&[&row], &tasks);
        assert!(!groups.has_named_initiative());
        assert_eq!(groups.initiatives.len(), 1, "one no-initiative bucket");
    }

    #[test]
    fn defined_but_empty_projects_render_and_initiatives_bucket_with_none_last() {
        let row = repo_row(
            vec![
                task("TASK-1", Some("Alpha"), true),
                task("TASK-2", Some("Beta"), false),
                task("TASK-3", None, false),
            ],
            vec![def("Alpha", Some("Big")), def("Empty", Some("Big"))],
            vec![],
        );
        let tasks = rows(&row);
        let groups = initiative_groups(&[&row], &tasks);

        assert!(groups.has_named_initiative());
        assert_eq!(groups.initiatives.len(), 2);
        let (initiative, projects) = &groups.initiatives[0];
        assert_eq!(initiative.name.as_deref(), Some("Big"));
        let names: Vec<&str> = projects.iter().map(|p| p.rollup.name.as_str()).collect();
        assert_eq!(names, vec!["Alpha", "Empty"], "0/0 project still appears");
        assert!(projects[1].rows.is_empty());
        assert!(
            projects[0].rollup.status.is_some() && !projects[0].rows.is_empty(),
            "def metadata and visible rows both reach the group"
        );
        let (bucket, bucket_projects) = &groups.initiatives[1];
        assert_eq!(bucket.name, None, "no-initiative bucket last");
        assert_eq!(bucket_projects[0].rollup.name, "Beta");
        assert_eq!(groups.unassigned.len(), 1, "unassigned tasks separate");
    }

    /// The lens joins *visible* rows onto repo-wide structure: a project
    /// whose rows are all filtered out still appears (from the rollup),
    /// with zero visible rows.
    #[test]
    fn a_fully_filtered_project_still_appears_with_no_rows() {
        let row = repo_row(vec![task("TASK-1", Some("Alpha"), false)], vec![], vec![]);
        let no_visible_rows: Vec<TaskRow<'_>> = Vec::new();
        let groups = initiative_groups(&[&row], &no_visible_rows);
        let (_, projects) = &groups.initiatives[0];
        assert_eq!(projects[0].rollup.name, "Alpha");
        assert!(projects[0].rows.is_empty());
    }
}
