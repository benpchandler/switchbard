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

use super::{format, Pending, RepoRow, Snapshot, TaskRow};
use crate::app::HiveApp;
use crate::ui::components::{status_pill, StatusKind};
use crate::ui::theme;
use eframe::egui;
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use switchbard_core::{compute_hierarchy_rollup, InitiativeRollup, ProjectRollup, RankMove};

const UNASSIGNED_LABEL: &str = "Unassigned";
const PROGRESS_BAR_WIDTH: f32 = 120.0;

/// One project's group for this frame: the core rollup entry (structure +
/// def metadata) joined to the *visible* task rows, plus the stack-rank
/// facts its header controls need (trajectory: *Stack ranking*).
struct ProjectGroup<'a> {
    rollup: ProjectRollup,
    rows: Vec<&'a TaskRow<'a>>,
    /// The repo root a rank arrow writes to — the first scoped repo where
    /// this name is live. `None` for a name no scoped repo knows (cannot
    /// happen for rollup-born groups, but Rule 5 says don't assume).
    rank_root: Option<PathBuf>,
    /// The name's position in its owning repo's ranked project list
    /// (raw — see `RepoRanking::task_rank_position`'s caveat).
    rank: Option<usize>,
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

    // Each repo's computed rank order is already applied to its
    // `repo.tasks` by `load_backlog_repo`, so a task's index there IS its
    // place in the repo-wide flatten. Rows inside a group re-sort by it —
    // the group order is the rank order, whatever the toolbar sort key says
    // about the flat List lens.
    let task_order: HashMap<&Path, HashMap<&str, usize>> = scoped
        .iter()
        .map(|repo| {
            (
                repo.key.as_path(),
                repo.repo
                    .tasks
                    .iter()
                    .enumerate()
                    .map(|(index, task)| (task.id.as_str(), index))
                    .collect(),
            )
        })
        .collect();
    let computed_position = |row: &&TaskRow<'_>| {
        task_order
            .get(row.repo.key.as_path())
            .and_then(|by_id| by_id.get(row.task.id.as_str()))
            .copied()
            .unwrap_or(usize::MAX)
    };

    let mut rows_by_project: BTreeMap<&str, Vec<&TaskRow<'_>>> = BTreeMap::new();
    let mut unassigned: Vec<&TaskRow<'_>> = Vec::new();
    for row in tasks {
        match row.task.project.as_deref() {
            Some(project) => rows_by_project.entry(project).or_default().push(row),
            None => unassigned.push(row),
        }
    }
    for rows in rows_by_project.values_mut() {
        rows.sort_by_key(computed_position);
    }
    unassigned.sort_by_key(computed_position);

    // A rank arrow needs one owning repo to write to; name-merged groups
    // take the first scoped repo where the name is live (deterministic —
    // scoped order is stable), and its rank position for the arrows.
    let rank_facts = |name: &str| -> (Option<PathBuf>, Option<usize>) {
        for repo in scoped {
            let live = repo
                .repo
                .tasks
                .iter()
                .any(|task| task.project.as_deref() == Some(name))
                || repo.repo.project_defs.iter().any(|def| def.name == name);
            if live {
                return (Some(repo.key.clone()), repo.repo.ranking.project_rank(name));
            }
        }
        (None, None)
    };

    let initiatives = rollup
        .initiatives
        .into_iter()
        .map(|initiative| {
            let mut projects: Vec<ProjectGroup<'_>> = initiative
                .projects
                .iter()
                .map(|project| {
                    let (rank_root, rank) = rank_facts(&project.name);
                    ProjectGroup {
                        rows: rows_by_project
                            .remove(project.name.as_str())
                            .unwrap_or_default(),
                        rollup: project.clone(),
                        rank_root,
                        rank,
                    }
                })
                .collect();
            // Ranked projects lead in rank order; the unranked rest keep
            // the rollup's name sort (`sort_by_key` is stable).
            projects.sort_by_key(|group| group.rank.unwrap_or(usize::MAX));
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
    pending: &mut Pending,
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
                    render_initiative(app, ui, initiative, projects, show_repo, pending);
                } else {
                    for project in projects {
                        render_project(app, ui, project, show_repo, pending);
                    }
                }
            }
            if !groups.unassigned.is_empty() {
                render_unassigned(app, ui, &groups.unassigned, show_repo, pending);
            }
        });
}

fn render_initiative(
    app: &mut HiveApp,
    ui: &mut egui::Ui,
    initiative: &InitiativeRollup,
    projects: &[ProjectGroup<'_>],
    show_repo: bool,
    pending: &mut Pending,
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
            render_project(app, ui, project, show_repo, pending);
        }
    });
}

fn render_project(
    app: &mut HiveApp,
    ui: &mut egui::Ui,
    project: &ProjectGroup<'_>,
    show_repo: bool,
    pending: &mut Pending,
) {
    let done = project.rows.iter().filter(|r| r.task.is_done()).count();
    let total = project.rows.len();
    egui::CollapsingHeader::new(format!("{}  ·  {done}/{total} done", project.rollup.name))
        .default_open(true)
        .id_salt(format!("project_{}", project.rollup.name))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                if let Some(rank_root) = &project.rank_root {
                    if let Some(direction) =
                        rank_arrows(ui, project.rank, "project", "the repo's projects")
                    {
                        pending.rank_move_project =
                            Some((rank_root.clone(), project.rollup.name.clone(), direction));
                    }
                }
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
                render_row(app, ui, row, show_repo, pending);
            }
        });
}

/// The Unassigned bucket: plain task group, no def metadata or progress bar
/// — it's the catch-all, not a project anyone tracks progress toward.
fn render_unassigned(
    app: &mut HiveApp,
    ui: &mut egui::Ui,
    rows: &[&TaskRow<'_>],
    show_repo: bool,
    pending: &mut Pending,
) {
    let done = rows.iter().filter(|row| row.task.is_done()).count();
    let total = rows.len();
    egui::CollapsingHeader::new(format!("{UNASSIGNED_LABEL}  ·  {done}/{total} done"))
        .default_open(true)
        .id_salt(format!("project_{UNASSIGNED_LABEL}"))
        .show(ui, |ui| {
            for row in rows {
                render_row(app, ui, row, show_repo, pending);
            }
        });
}

/// The ▲▼ pair every rankable thing gets, with the sparse-rank arrow
/// semantics from `switchbard_core::RankMove` spelled out on hover.
/// Returns the direction of a click, if any. `rank` is the item's raw
/// position in its ranked sibling list (`None` = unranked): up is disabled
/// only at the top; down is disabled while unranked.
fn rank_arrows(
    ui: &mut egui::Ui,
    rank: Option<usize>,
    noun: &str,
    scope_label: &str,
) -> Option<RankMove> {
    let can_up = rank != Some(0);
    let can_down = rank.is_some();
    let mut clicked = None;
    let up = theme::triangle_button(ui, true, can_up);
    if can_up {
        let hover = if rank.is_some() {
            format!("Move this {noun} up one rank")
        } else {
            format!("Rank this {noun} (enters the bottom of {scope_label}' ranked list)")
        };
        if up.on_hover_text(hover).clicked() {
            clicked = Some(RankMove::Up);
        }
    } else {
        up.on_hover_text(format!("Already the top-ranked {noun}"));
    }
    let down = theme::triangle_button(ui, false, can_down);
    if can_down {
        let hover = format!(
            "Move this {noun} down one rank (moving the lowest-ranked {noun} down unranks it)"
        );
        if down.on_hover_text(hover).clicked() {
            clicked = Some(RankMove::Down);
        }
    } else {
        down.on_hover_text(format!(
            "Unranked — an unranked {noun} sorts by the computed fallback; rank it to move it"
        ));
    }
    clicked
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

fn render_row(
    app: &mut HiveApp,
    ui: &mut egui::Ui,
    row: &TaskRow<'_>,
    show_repo: bool,
    pending: &mut Pending,
) {
    let key = row.key();
    let selected = app.backlog_view.selected_task.as_ref() == Some(&key);
    let rankable = row.task.editable() && !row.task.is_done();
    ui.horizontal(|ui| {
        if rankable {
            let rank = row.repo.repo.ranking.task_rank_position(row.task);
            if let Some(direction) = rank_arrows(ui, rank, "task", "its sibling scope") {
                pending.rank_move_task =
                    Some((row.repo.key.clone(), row.task.id.clone(), direction));
            }
        } else {
            // Keep finished rows column-aligned with rankable ones.
            ui.add_space(2.0 * theme::ICON_SIZE + ui.spacing().item_spacing.x);
        }
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
        if row.repo.repo.ranking.is_expedited(&row.task.id) {
            status_pill(
                ui,
                StatusKind::Danger,
                "expedited",
                Some("In the expedite lane — jumps the repo's whole computed order"),
            );
        }
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
                ranking: switchbard_core::RepoRanking::default(),
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

    /// Stack ranking (trajectory: *Stack ranking*): ranked projects lead
    /// their initiative group in rank order; the unranked rest keep the
    /// rollup's name sort. The rank arrows get the owning repo and the
    /// raw rank position.
    #[test]
    fn ranked_projects_lead_their_group_and_carry_rank_facts() {
        let mut row = repo_row(
            vec![
                task("TASK-1", Some("Alpha"), false),
                task("TASK-2", Some("Beta"), false),
                task("TASK-3", Some("Zulu"), false),
            ],
            vec![],
            vec![],
        );
        row.repo.ranking.projects = vec!["Zulu".to_string()];
        let tasks = rows(&row);
        let groups = initiative_groups(&[&row], &tasks);

        let (_, projects) = &groups.initiatives[0];
        let names: Vec<&str> = projects.iter().map(|p| p.rollup.name.as_str()).collect();
        assert_eq!(names, vec!["Zulu", "Alpha", "Beta"]);
        assert_eq!(projects[0].rank, Some(0));
        assert_eq!(projects[0].rank_root.as_deref(), Some(row.key.as_path()));
        assert_eq!(projects[1].rank, None, "unranked projects report no rank");
    }

    /// Rows inside a group follow the repo's computed order (`repo.tasks`
    /// position — the authority `load_backlog_repo` sorted), not the order
    /// the visible rows happened to arrive in from the toolbar sort.
    #[test]
    fn group_rows_follow_the_repos_computed_order() {
        let row = repo_row(
            vec![
                task("TASK-9", Some("Alpha"), false),
                task("TASK-1", Some("Alpha"), false),
                task("TASK-5", None, false),
                task("TASK-2", None, false),
            ],
            vec![],
            vec![],
        );
        // Visible rows arrive in a different (e.g. priority-sorted) order.
        let mut tasks = rows(&row);
        tasks.reverse();
        let groups = initiative_groups(&[&row], &tasks);

        let (_, projects) = &groups.initiatives[0];
        let alpha_ids: Vec<&str> = projects[0]
            .rows
            .iter()
            .map(|r| r.task.id.as_str())
            .collect();
        assert_eq!(alpha_ids, vec!["TASK-9", "TASK-1"], "repo.tasks order wins");
        let unassigned_ids: Vec<&str> = groups
            .unassigned
            .iter()
            .map(|r| r.task.id.as_str())
            .collect();
        assert_eq!(unassigned_ids, vec!["TASK-5", "TASK-2"]);
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
