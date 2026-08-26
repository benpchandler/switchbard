//! Backlog project-management view.
//!
//! The view renders cached `backlog/` task snapshots for every tracked
//! worktree that has a Backlog project. Mutations go back through the
//! `backlog` CLI from worker threads; this module only queues intents.
//!
//! ## Unified scope
//!
//! `app.backlog_view.selected_project` doubles as the scope switch: `None`
//! (the default) is "All projects" — every tracked project's tasks merged
//! into one triage-ranked list (`sort::visible_task_rows` +
//! `switchbard_core::triage_rank`), each row tagged with a repo badge and
//! `repo:id`-formatted task id. `Some(path)` narrows to that one project,
//! matching how the view worked before the unified scope. See `sort.rs` for
//! the ranking/filtering pipeline and `runtime::BacklogTaskKey` for why
//! selection is keyed on `(project_root, task_id)` rather than a bare id —
//! ids are only unique within a project.
//!
//! ## Module map
//! - `format`     — presentation-only helpers (status/priority labels+colors).
//! - `toolbar`    — summary line + project/status/priority filter bar + lens tabs.
//! - `sort`       — task filtering, sorting (incl. triage ranking), status options.
//! - `selection`  — bulk multi-select state machine (shift/ctrl-click, context menu targets).
//! - `detail_lists` — detail-pane checklist/list sections split out of `detail`.
//! - `digest`     — the Digest lens: "what should I do today" landing screen (task-21).
//! - `dispatch_ui` — dispatch state derivation + pill, shared by detail/list/board.
//! - `list`       — the List lens: task list column + row rendering.
//! - `tree`        — the List lens's sub-task tree walk, split out of `list` (task-17).
//! - `board`      — the Board lens: per-status kanban columns with drag-to-change-status.
//! - `milestones` — the Milestones lens: tasks grouped by milestone, cross-repo.
//! - `portfolio`  — the Portfolio lens: read-only per-repo health (task-19).
//! - `stats`      — the Statistics lens: cross-repo totals + burndown (task-16).
//! - `search`     — the Cmd+K / Ctrl+K global free-text search overlay.
//! - `saved_views` — named filter+sort+lens combinations, persisted on `Config::ui` (task-20).
//! - `detail`     — the selected-task detail pane (editor, acceptance, notes).
//! - `create`     — the "New Backlog Task" modal.
//! - `rail`       — the persistent right-hand detail rail (owner UX pass, 2026-08-05).
//!
//! ## Lens
//!
//! `app.backlog_view.lens` (`BacklogLens`) picks which of the six central
//! renderers below the toolbar owns the frame: `Digest` (default), `List`,
//! `Board`, `Milestones`, `Portfolio`, or `Statistics`. All six share the
//! same `Snapshot`; `List`/`Board` additionally share one triage-ranked/
//! filtered `tasks` list computed once per frame in `render` (see the perf
//! note there) — `Digest`/`Portfolio`/`Statistics` read `scoped_projects`
//! directly instead, since a read-only aggregation or landing page
//! shouldn't go empty just because the toolbar's Done/Archived filters are
//! off elsewhere.
//!
//! ## Selecting a task shows its detail, regardless of lens
//!
//! Owner UX pass (2026-08-05): every lens's "click a task" affordance
//! (Board card, List row, Digest card, Milestones row, search result) sets
//! `backlog_view.selected_task` and nothing else — no lens switch. `rail`
//! renders that selection's detail in a persistent right-side panel
//! alongside whichever lens is active, replacing the old behavior where a
//! click jumped you to the List lens specifically (its own detail pane was
//! the only place selection was visible). List no longer embeds its own
//! detail split for the same reason — the rail is the one place detail
//! renders now, reusing `detail::render_task_detail` unchanged.

mod board;
mod create;
mod detail;
mod detail_lists;
mod digest;
pub(crate) mod dispatch_ui;
mod format;
mod list;
mod milestones;
mod portfolio;
mod rail;
mod saved_views;
mod search;
mod selection;
mod sort;
mod stats;
mod toolbar;
mod tree;

use crate::app::HiveApp;
use crate::runtime::worktree_names::worktree_display_name;
use crate::runtime::{BacklogLens, BacklogTaskKey};
use crate::ui::theme;
use eframe::egui;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use switchbard_core::{BacklogProject, BacklogTask, BacklogTaskPatch, NewBacklogTask, Repo};

pub(in crate::ui::backlog) struct ProjectRow {
    pub key: PathBuf,
    pub project: BacklogProject,
    pub repo_name: String,
    pub worktree_label: String,
    pub branch: Option<String>,
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

/// One task paired with the project it belongs to — the unit the list/detail
/// panes render, regardless of whether the view is scoped to one project or
/// merging all of them.
pub(in crate::ui::backlog) struct TaskRow<'a> {
    pub project: &'a ProjectRow,
    pub task: &'a BacklogTask,
}

impl TaskRow<'_> {
    pub fn key(&self) -> BacklogTaskKey {
        (self.project.key.clone(), self.task.id.clone())
    }
}

pub(in crate::ui::backlog) struct Snapshot {
    pub projects: Vec<ProjectRow>,
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

        Self { projects: rows }
    }

    /// Look up one project by its worktree-root key, regardless of scope.
    /// Used wherever a single project is needed even while the view as a
    /// whole is scoped to "All projects" (task creation target, detail pane).
    pub(super) fn project(&self, key: &Path) -> Option<&ProjectRow> {
        self.projects.iter().find(|row| row.key == key)
    }
}

/// The project rows the task list draws from for the current scope: every
/// tracked project in "All projects" scope, or just the selected one.
/// Whether a lens is driven by the filter row.
///
/// One definition, used both to decide whether to render the filters and
/// whether the summary line may claim "N of M" — the two must not disagree
/// or the header would describe a filter the user cannot see.
pub(super) fn lens_filters(lens: BacklogLens) -> bool {
    matches!(
        lens,
        BacklogLens::List | BacklogLens::Board | BacklogLens::Milestones
    )
}

pub(in crate::ui::backlog) fn scoped_projects<'a>(
    app: &HiveApp,
    snap: &'a Snapshot,
) -> Vec<&'a ProjectRow> {
    match &app.backlog_view.selected_project {
        None => snap.projects.iter().collect(),
        Some(path) => snap
            .projects
            .iter()
            .filter(|row| &row.key == path)
            .collect(),
    }
}

pub fn render(app: &mut HiveApp, ui: &mut egui::Ui) {
    let ctx = &ui.ctx().clone();
    let snap = Snapshot::collect(app);
    reconcile_selected_project(app, &snap);
    // Computed once per frame — it re-sorts (and, for the default Triage key,
    // re-ranks via a per-task clone into `TriageEntry`) every visible task,
    // so a second pass here is exactly the "per-frame rebuild" render-path
    // perf rule warns against. `reconcile_selected_task` and every lens
    // renderer below share this one list.
    let tasks = sort::visible_task_rows(app, &snap);
    reconcile_selected_task(app, &tasks);
    let mut pending = Pending::default();

    search::handle_shortcut(app, ctx);

    // Must render before the CentralPanel below so it claims its docked
    // space first (same ordering rule every side panel in this app follows
    // — see `HiveApp::render_ui`'s own comment). Skipped when there's
    // nothing to ever select, matching the CentralPanel's own empty-scope
    // branch just below.
    if !snap.projects.is_empty() {
        rail::render_detail_rail(app, ui, &snap, &mut pending);
    }

    let workspace_frame =
        egui::Frame::central_panel(&ctx.style_of(ctx.theme())).inner_margin(egui::Margin::same(12));
    egui::CentralPanel::default()
        .frame(workspace_frame)
        .show(ui, |ui| {
            if snap.projects.is_empty() {
                render_empty(ui);
                return;
            }
            // Only the lenses that actually apply the filter bar can claim
            // "N of M"; Digest and Statistics summarise the whole scope.
            let visible_count = lens_filters(app.backlog_view.lens).then_some(tasks.len());
            toolbar::render_summary(app, ui, &snap, &mut pending, visible_count);
            ui.add_space(6.0);
            // One container for the whole control surface: lens tabs, the
            // filter row, and saved views. These used to carry three
            // different treatments — a bordered tab strip, an unframed
            // saved-views row, and a second bordered filter panel — which is
            // most of what read as clutter above the board.
            egui::Frame::default()
                .fill(theme::nav_bg())
                .stroke(theme::surface_stroke())
                .corner_radius(6.0)
                .inner_margin(egui::Margin::symmetric(10, 7))
                .show(ui, |ui| {
                    toolbar::render_lens_tabs(app, ui);
                    if lens_filters(app.backlog_view.lens) {
                        ui.separator();
                        toolbar::render_project_toolbar(app, ui, &snap);
                        ui.separator();
                        list::render_task_sort_controls(app, ui);
                    }
                    ui.separator();
                    saved_views::render_saved_views_bar(app, ui);
                });
            match app.backlog_view.lens {
                BacklogLens::Digest => {
                    ui.separator();
                    digest::render_digest(app, ui, &snap);
                }
                BacklogLens::List => {
                    ui.separator();
                    list::render_task_workspace(app, ui, tasks, &mut pending);
                }
                BacklogLens::Board => {
                    ui.separator();
                    board::render_board(app, ui, &snap, tasks, &mut pending);
                }
                BacklogLens::Milestones => {
                    ui.separator();
                    milestones::render_milestones(app, ui, &snap, tasks);
                }
                BacklogLens::Portfolio => {
                    ui.separator();
                    portfolio::render_portfolio(app, ui, &snap);
                }
                BacklogLens::Statistics => {
                    ui.separator();
                    stats::render_statistics(app, ui, &snap);
                }
            }
        });

    search::render_overlay(app, ctx, &snap);
    create::render_create_modal(app, ctx, &snap, &mut pending);
    apply_pending(app, ui, pending);
}

/// Fall back to "All projects" if the scoped project vanished (repo
/// untracked, or no projects left at all) — never guess a replacement.
fn reconcile_selected_project(app: &mut HiveApp, snap: &Snapshot) {
    if snap.projects.is_empty() {
        app.backlog_view.selected_project = None;
        reset_task_selection(app);
        return;
    }
    if let Some(path) = app.backlog_view.selected_project.clone() {
        if !snap.projects.iter().any(|row| row.key == path) {
            app.backlog_view.selected_project = None;
            reset_task_selection(app);
        }
    }
}

/// Fall back to the first visible task if the detail selection scrolled out
/// of view (a filter changed, or the task itself vanished).
fn reconcile_selected_task(app: &mut HiveApp, visible: &[TaskRow<'_>]) {
    let current_visible = app
        .backlog_view
        .selected_task
        .as_ref()
        .is_some_and(|key| visible.iter().any(|row| &row.key() == key));
    if !current_visible {
        app.backlog_view.selected_task = visible.first().map(|row| row.key());
        app.backlog_view.editor.loaded_key = None;
    }
}

/// Clear task-level selection state (detail selection, bulk selection, the
/// in-progress editor buffer). Does not touch `selected_project` — callers
/// that are changing scope set that separately.
pub(in crate::ui::backlog) fn reset_task_selection(app: &mut HiveApp) {
    app.backlog_view.selected_task = None;
    app.backlog_view.bulk_selected_tasks.clear();
    app.backlog_view.bulk_selection_anchor = None;
    app.backlog_view.editor.loaded_key = None;
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
            .color(theme::muted_text()),
        );
    });
}

#[derive(Default)]
pub(in crate::ui::backlog) struct Pending {
    pub save: Option<(PathBuf, String, BacklogTaskPatch)>,
    /// One entry per project a bulk action touches — a cross-repo bulk
    /// selection needs one `backlog` CLI invocation per project root.
    pub bulk_save: Vec<(PathBuf, Vec<String>, BacklogTaskPatch, String)>,
    pub toggle_ac: Option<(PathBuf, String, usize, bool)>,
    pub toggle_dod: Option<(PathBuf, String, usize, bool)>,
    pub append_note: Option<(PathBuf, String, String)>,
    pub create: Option<(PathBuf, NewBacklogTask)>,
    pub archive: Option<(PathBuf, String)>,
    /// The Done-task counterpart to `archive` — `detail_lists::
    /// render_archive` routes here instead when `task.is_done()`, since the
    /// real CLI refuses `task archive` on a Done task.
    pub complete: Option<(PathBuf, String)>,
    /// `(project_root, task_id)` — the per-task Refine grooming pass
    /// (TASK-44). Unlike `dispatch_toggle`, which only flags a label for a
    /// worker to notice later, this queues the whole run: `HiveApp::
    /// spawn_backlog_refine` does the headless call and the additive write
    /// on its own thread.
    pub refine: Option<(PathBuf, String)>,
    /// `(project_root, task_id, enabled)` — the per-task Dispatch opt-in
    /// toggle (task-11 GUI wiring).
    pub dispatch_toggle: Option<(PathBuf, String, bool)>,
    /// "Clean Up Old Tasks" (QA parity matrix LOW gap): one entry per
    /// project with Done tasks to archive, same cross-repo shape as
    /// `bulk_save` — a bulk archive still needs one `backlog` CLI
    /// invocation per task, and those are scattered across every tracked
    /// project, not just one.
    pub cleanup: Option<Vec<(PathBuf, Vec<String>)>>,
    /// A bulk clear off the active board, already split by disposition:
    /// Done tasks are completed, the rest archived. See
    /// `toolbar::ClearBatch` for why both halves travel together.
    pub bulk_clear: Option<toolbar::ClearBatch>,
}

fn apply_pending(app: &mut HiveApp, ui: &mut egui::Ui, pending: Pending) {
    let ctx = &ui.ctx().clone();
    if let Some((project_root, task_id, patch)) = pending.save {
        app.spawn_backlog_save(project_root, task_id, patch, ctx);
    }
    for (project_root, task_ids, patch, action_label) in pending.bulk_save {
        app.spawn_backlog_bulk_save(project_root, task_ids, patch, action_label, ctx);
    }
    if let Some((project_root, task_id, index, checked)) = pending.toggle_ac {
        app.spawn_backlog_acceptance_toggle(project_root, task_id, index, checked, ctx);
    }
    if let Some((project_root, task_id, index, checked)) = pending.toggle_dod {
        app.spawn_backlog_dod_toggle(project_root, task_id, index, checked, ctx);
    }
    if let Some((project_root, task_id, note)) = pending.append_note {
        app.spawn_backlog_append_note(project_root, task_id, note, ctx);
    }
    if let Some((project_root, task)) = pending.create {
        app.spawn_backlog_create(project_root, task, ctx);
    }
    if let Some((project_root, task_id)) = pending.archive {
        app.spawn_backlog_archive(project_root, task_id, ctx);
    }
    if let Some((project_root, task_id)) = pending.complete {
        app.spawn_backlog_complete(project_root, task_id, ctx);
    }
    if let Some((project_root, task_id)) = pending.refine {
        app.spawn_backlog_refine(project_root, task_id, ctx);
    }
    if let Some((project_root, task_id, enabled)) = pending.dispatch_toggle {
        app.spawn_backlog_dispatch_toggle(project_root, task_id, enabled, ctx);
    }
    if let Some(per_project) = pending.cleanup {
        app.spawn_backlog_cleanup(per_project, ctx);
    }
    if let Some(batch) = pending.bulk_clear {
        app.spawn_backlog_bulk_clear(batch.archive, batch.complete, ctx);
    }
}
