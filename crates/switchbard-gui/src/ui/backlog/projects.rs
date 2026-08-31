//! The Projects lens: every visible task grouped by `BacklogTask::project`,
//! cross-repo, with projects nested under their initiative (from the
//! project's def file) and an "Unassigned" bucket last. Project *assignment*
//! lives in the detail pane (`detail::render_editor`); def lifecycle is
//! CLI-first (`switchbard-task project create/edit`); this lens is
//! read/browse-only — clicking a task selects it, which the persistent
//! detail rail shows regardless of lens.
//!
//! Grouping is pure ([`initiative_groups`]) over the frame's already-loaded
//! snapshot — def lookups come from `BacklogRepo::{project_defs,
//! initiative_defs}`, which ride the backlog worker's snapshot, so this lens
//! does no IO. When no named initiative exists anywhere in view, the
//! initiative header level is skipped entirely: a lone "No initiative"
//! wrapper around everything would be pure noise.

use super::{format, RepoRow, Snapshot, TaskRow};
use crate::app::HiveApp;
use crate::ui::components::{status_pill, StatusKind};
use crate::ui::theme;
use eframe::egui;
use std::collections::BTreeMap;
use switchbard_core::{InitiativeDef, ProjectDef};

const UNASSIGNED_LABEL: &str = "Unassigned";
const PROGRESS_BAR_WIDTH: f32 = 120.0;

/// One project's group for this frame: its visible task rows plus its def,
/// when one exists in any scoped repo.
struct ProjectGroup<'a> {
    name: String,
    def: Option<&'a ProjectDef>,
    rows: Vec<&'a TaskRow<'a>>,
}

/// The frame's full grouping: initiatives (name-sorted, `None` bucket last)
/// each holding name-sorted project groups, plus the unassigned rows.
struct Groups<'a> {
    initiatives: Vec<(
        Option<&'a str>,
        Option<&'a InitiativeDef>,
        Vec<ProjectGroup<'a>>,
    )>,
    unassigned: Vec<&'a TaskRow<'a>>,
}

impl Groups<'_> {
    fn is_empty(&self) -> bool {
        self.initiatives.is_empty() && self.unassigned.is_empty()
    }

    /// Whether the initiative header level carries any information — false
    /// when the only bucket is the no-initiative one.
    fn has_named_initiative(&self) -> bool {
        self.initiatives.iter().any(|(name, _, _)| name.is_some())
    }
}

fn initiative_groups<'a>(scoped: &[&'a RepoRow], tasks: &'a [TaskRow<'a>]) -> Groups<'a> {
    // First def wins on cross-repo name conflicts — same deterministic rule
    // as `compute_hierarchy_rollup`.
    let mut project_defs: BTreeMap<&str, &ProjectDef> = BTreeMap::new();
    let mut initiative_defs: BTreeMap<&str, &InitiativeDef> = BTreeMap::new();
    for repo in scoped {
        for def in &repo.repo.project_defs {
            project_defs.entry(def.name.as_str()).or_insert(def);
        }
        for def in &repo.repo.initiative_defs {
            initiative_defs.entry(def.name.as_str()).or_insert(def);
        }
    }

    let mut by_project: BTreeMap<String, Vec<&TaskRow<'_>>> = BTreeMap::new();
    let mut unassigned: Vec<&TaskRow<'_>> = Vec::new();
    for row in tasks {
        match &row.task.project {
            Some(project) => by_project.entry(project.clone()).or_default().push(row),
            None => unassigned.push(row),
        }
    }
    // Def-declared projects with no visible tasks still render (0/0) — that
    // is what makes `project create` visible before assignment.
    for name in project_defs.keys() {
        by_project.entry((*name).to_string()).or_default();
    }

    let mut by_initiative: BTreeMap<Option<&str>, Vec<ProjectGroup<'_>>> = BTreeMap::new();
    for (name, rows) in by_project {
        let def = project_defs.get(name.as_str()).copied();
        by_initiative
            .entry(def.and_then(|d| d.initiative.as_deref()))
            .or_default()
            .push(ProjectGroup { name, def, rows });
    }

    // `Option` sorts `None` first; the no-initiative bucket belongs last.
    let mut initiatives = Vec::with_capacity(by_initiative.len());
    let mut bucket = None;
    for (name, groups) in by_initiative {
        let def = name.and_then(|n| initiative_defs.get(n).copied());
        if name.is_none() {
            bucket = Some((name, def, groups));
        } else {
            initiatives.push((name, def, groups));
        }
    }
    initiatives.extend(bucket);

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
            for (initiative, def, projects) in &groups.initiatives {
                if nest_under_initiatives {
                    render_initiative(app, ui, initiative.as_deref(), *def, projects, show_repo);
                } else {
                    for project in projects {
                        render_project(app, ui, project, show_repo);
                    }
                }
            }
            if !groups.unassigned.is_empty() {
                render_task_group(app, ui, UNASSIGNED_LABEL, &groups.unassigned, show_repo);
            }
        });
}

fn render_initiative(
    app: &mut HiveApp,
    ui: &mut egui::Ui,
    name: Option<&str>,
    def: Option<&InitiativeDef>,
    projects: &[ProjectGroup<'_>],
    show_repo: bool,
) {
    let label = name.unwrap_or("No initiative");
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
        if let Some(def) = def {
            header_meta(ui, &def.status, def.target_date.as_deref());
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
    egui::CollapsingHeader::new(format!("{}  ·  {done}/{total} done", project.name))
        .default_open(true)
        .id_salt(format!("project_{}", project.name))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                if let Some(def) = project.def {
                    status_pill(ui, project_status_kind(&def.status), &def.status, None);
                    if let Some(target) = def.target_date.as_deref() {
                        ui.label(
                            egui::RichText::new(format!("target {target}"))
                                .small()
                                .color(theme::muted_text()),
                        );
                    }
                }
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
fn render_task_group(
    app: &mut HiveApp,
    ui: &mut egui::Ui,
    label: &str,
    rows: &[&TaskRow<'_>],
    show_repo: bool,
) {
    let done = rows.iter().filter(|row| row.task.is_done()).count();
    let total = rows.len();
    egui::CollapsingHeader::new(format!("{label}  ·  {done}/{total} done"))
        .default_open(true)
        .id_salt(format!("project_{label}"))
        .show(ui, |ui| {
            for row in rows {
                render_row(app, ui, row, show_repo);
            }
        });
}

fn header_meta(ui: &mut egui::Ui, status: &str, target_date: Option<&str>) {
    ui.horizontal(|ui| {
        status_pill(ui, project_status_kind(status), status, None);
        if let Some(target) = target_date {
            ui.label(
                egui::RichText::new(format!("target {target}"))
                    .small()
                    .color(theme::muted_text()),
            );
        }
    });
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
    use switchbard_core::{BacklogRepo, BacklogTask, BacklogTaskSource};

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

    fn rows<'a>(row: &'a RepoRow) -> Vec<TaskRow<'a>> {
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
        let (name, _, projects) = &groups.initiatives[0];
        assert_eq!(name.as_deref(), Some("Big"));
        let names: Vec<&str> = projects.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, vec!["Alpha", "Empty"], "0/0 project still appears");
        assert!(projects[1].rows.is_empty());
        let (bucket_name, _, bucket_projects) = &groups.initiatives[1];
        assert_eq!(*bucket_name, None, "no-initiative bucket last");
        assert_eq!(bucket_projects[0].name, "Beta");
        assert_eq!(groups.unassigned.len(), 1, "unassigned tasks separate");
    }
}
