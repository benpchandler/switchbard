//! The summary line ("N open · M total", warnings) and the filter bar
//! (repo/scope picker, status/priority filters, completed/archived
//! toggles).

use super::{create, format, reset_task_selection, sort, Pending, Snapshot};
use crate::app::HiveApp;
use crate::runtime::BacklogLens;
use crate::sync::BulkProgress;
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
                BacklogLens::Projects,
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
/// Statistics), where "N of M" would be a claim about nothing. `heading` —
/// TASK-97: the Tasks place reuses this for its own title row and needs
/// "Tasks" (the mock's own heading), not the legacy "Backlog" every other
/// call site still wants.
pub(crate) fn render_summary(
    app: &mut HiveApp,
    ui: &mut egui::Ui,
    snap: &Snapshot,
    pending: &mut Pending,
    visible_count: Option<usize>,
    heading: &str,
) {
    let scoped = super::scoped_repos(app, snap);
    let task_count: usize = scoped.iter().map(|row| row.repo.tasks.len()).sum();
    let open_count: usize = scoped
        .iter()
        .map(|row| sort::open_task_count(&row.repo))
        .sum();
    let warning_count: usize = scoped.iter().map(|row| row.repo.warnings.len()).sum();
    let ordering_warning = app.ordering_snapshot().warning;

    // Below ~640px (mock §7d's narrow-width stress state, matching
    // `render_project_toolbar`'s own `compact` threshold) the heading/count
    // text and the right-aligned action buttons no longer both fit on one
    // line. A plain `ui.horizontal` doesn't reflow when that happens — its
    // left-to-right cursor and a nested `right_to_left` layout each lay out
    // against the row's *original* full width independently, so instead of
    // wrapping they paint on top of each other (2026-09 parity audit
    // screenshot: "Completed: 0/6 showing" bleeding into "Clean Up Old
    // Tasks"). Stacking the info line and the action row separately below
    // the threshold gives each its own full-width budget instead of sharing
    // one that neither actually reflows within.
    let compact = ui.available_width() < 640.0;

    let render_info = |ui: &mut egui::Ui| {
        ui.label(egui::RichText::new(heading).heading().strong());
        ui.separator();
        // One count, and when a filter is narrowing the view it explains the
        // gap itself ("370 of 1509") rather than sitting next to a second,
        // unrelated number further down the toolbar. The scope is not
        // repeated here — the repo picker directly below already states
        // it, and saying "All repos" twice was the loudest redundancy in
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
                Some("One or more Backlog repos loaded with warnings"),
            );
        }
        if let Some(warning) = &ordering_warning {
            ui.separator();
            status_pill(ui, StatusKind::Warn, "ordering.yml", Some(warning.as_str()));
        }
    };

    if compact {
        ui.horizontal_wrapped(render_info);
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            render_toolbar_actions(app, ui, snap, pending);
        });
    } else {
        ui.horizontal(|ui| {
            render_info(ui);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                render_toolbar_actions(app, ui, snap, pending);
            });
        });
    }
}

/// "Refresh Backlog" / "+ Task" / the bulk-progress-or-cleanup-and-clear
/// cluster — split out of `render_summary` so the compact (stacked) and
/// wide (single-row, right-aligned) layouts above can both call the same
/// button set instead of duplicating it.
fn render_toolbar_actions(app: &mut HiveApp, ui: &mut egui::Ui, snap: &Snapshot, pending: &mut Pending) {
    if ui
        .button("Refresh Backlog")
        .on_hover_text("Reload Backlog tasks from tracked worktrees")
        .clicked()
    {
        app.backlog_kick.notify();
        app.backlog_status.set("refreshing Backlog repos");
    }
    if ui
        .button("+ Task")
        .on_hover_text("Create a task in a Backlog repo")
        .clicked()
    {
        let target = app
            .backlog_view
            .selected_repo
            .clone()
            .or_else(|| snap.repos.first().map(|row| row.key.clone()));
        create::open_new_task(app, target, None);
    }

    // While a bulk run is live the bar takes the buttons' place rather than
    // sitting beside them: both actions mutate the same task set through
    // the same one-CLI-call-per-task loop, so offering to start a second
    // one mid-run is offering a race.
    if let Some(progress) = app.bulk_progress.snapshot() {
        render_bulk_progress(ui, &progress);
    } else {
        render_cleanup_button(app, ui, snap, pending);
        render_bulk_clear_button(app, ui, snap, pending);
    }
}

/// "Clean Up Old Tasks" (QA parity matrix LOW gap): complete every Done,
/// active, CLI-editable task across every tracked repo — a
/// workspace-wide housekeeping action, so it lives in the always-visible
/// summary line rather than the List lens's own toolbar, and always spans
/// every repo regardless of the current filter/scope ("cross-repo
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
        .on_hover_text("Complete every Done task across all tracked repos")
        .clicked()
    {
        app.backlog_view.cleanup_confirm = true;
    }
}

/// Every repo's Done, still-active task ids — the candidate set
/// `render_cleanup_button` completes. A `Completed`-sourced task (already
/// moved to `backlog/completed/`) or an already-`Archived`/`Draft` one is
/// excluded the same way a single task's Archive/Complete button already
/// requires `editable()`.
fn cleanup_candidates(snap: &Snapshot) -> Vec<(PathBuf, Vec<String>)> {
    snap.repos
        .iter()
        .filter_map(|row| {
            let ids: Vec<String> = row
                .repo
                .tasks
                .iter()
                .filter(|task| task.editable() && task.is_done())
                .map(|task| task.id.clone())
                .collect();
            (!ids.is_empty()).then_some((row.key.clone(), ids))
        })
        .collect()
}

/// The determinate bar for an in-flight bulk action.
///
/// Sized rather than left to fill the row: it lives inside the header's
/// right-to-left layout, where an unsized `ProgressBar` claims all remaining
/// width and shoves the heading off the other end.
///
/// Non-blocking by construction — it is an ordinary widget in a row that is
/// already there, so the rest of the app stays live and scrollable while a
/// sweep runs. No modal, no spinner overlay.
fn render_bulk_progress(ui: &mut egui::Ui, progress: &BulkProgress) {
    ui.add(
        egui::ProgressBar::new(progress.fraction())
            .desired_width(220.0)
            .text(progress.label()),
    )
    .on_hover_text("A bulk Backlog action is running; it is safe to keep working elsewhere");
}

/// A batch to clear off the active board, split by the disposition each
/// task actually needs.
///
/// Backlog.md's two terminal states are not interchangeable and the real CLI
/// refuses `task archive` on a Done task. A selection can legitimately span
/// both, so the batch carries both halves rather than silently dropping one.
#[derive(Default)]
pub(crate) struct ClearBatch {
    /// Open tasks → `backlog/archive/tasks/`.
    pub archive: Vec<(PathBuf, Vec<String>)>,
    /// Done tasks → `backlog/completed/`.
    pub complete: Vec<(PathBuf, Vec<String>)>,
}

impl ClearBatch {
    pub fn archive_count(&self) -> usize {
        self.archive.iter().map(|(_, ids)| ids.len()).sum()
    }

    pub fn complete_count(&self) -> usize {
        self.complete.iter().map(|(_, ids)| ids.len()).sum()
    }

    pub fn total(&self) -> usize {
        self.archive_count() + self.complete_count()
    }

    /// The verb to name this batch by. The button must never offer a verb it
    /// will not perform: an "Archive 15" that quietly completes three of them
    /// is a lie even though the routing is correct. The single-task control in
    /// the detail rail already renames itself this way; this is the same rule
    /// applied to a set.
    pub fn verb(&self) -> &'static str {
        match (self.archive_count(), self.complete_count()) {
            (0, _) => "Complete",
            (_, 0) => "Archive",
            _ => "Clear",
        }
    }

    /// Lowercase present participle for the progress bar.
    pub fn progress_verb(&self) -> &'static str {
        match (self.archive_count(), self.complete_count()) {
            (0, _) => "completing",
            (_, 0) => "archiving",
            _ => "clearing",
        }
    }
}

/// The tasks a bulk clear would act on, split by disposition.
///
/// Sourced from `sort::visible_task_rows` — the *same* function the lens
/// renders from — so the count can never drift from what is on screen. When
/// cards are ticked the batch is those cards instead: an explicit selection
/// is a narrower, more deliberate statement of intent than the filter, and it
/// may legitimately include Done cards the filtered path would have skipped.
///
/// Excludes tasks already in `archive/` or `completed/` (nowhere left to go).
pub(super) fn clear_batch(app: &HiveApp, snap: &Snapshot) -> ClearBatch {
    let selection = &app.backlog_view.bulk_selected_tasks;
    let mut archive: BTreeMap<PathBuf, Vec<String>> = BTreeMap::new();
    let mut complete: BTreeMap<PathBuf, Vec<String>> = BTreeMap::new();

    for row in sort::visible_task_rows(app, snap) {
        if matches!(
            row.task.source,
            BacklogTaskSource::Archived | BacklogTaskSource::Completed
        ) {
            continue;
        }
        if !selection.is_empty() && !selection.contains(&row.key()) {
            continue;
        }
        let bucket = if row.task.is_done() {
            &mut complete
        } else {
            &mut archive
        };
        bucket
            .entry(row.repo.key.clone())
            .or_default()
            .push(row.task.id.clone());
    }

    ClearBatch {
        archive: archive.into_iter().collect(),
        complete: complete.into_iter().collect(),
    }
}

/// The bulk clear control — the set counterpart to the per-task
/// Archive/Complete in the detail rail, and named the same way: by the verb
/// it will actually perform.
///
/// Two guards:
///
/// - **Only on a lens that shows the filter row.** The header renders on
///   every lens, but Digest/Portfolio/Statistics hide the filters, so the
///   count would describe a set the user cannot inspect or adjust.
/// - **With no selection, only when a filter narrows the view.** "Clear
///   what's showing" against an unfiltered board means the whole backlog.
///   An explicit tick-by-tick selection *is* that narrowing, so it lifts the
///   gate — the user named the set card by card.
///
/// Both dispositions are recoverable (files move between backlog/
/// directories, nothing is deleted), but a thousand accidental moves across
/// a dozen repos is still a bad afternoon.
fn render_bulk_clear_button(
    app: &mut HiveApp,
    ui: &mut egui::Ui,
    snap: &Snapshot,
    pending: &mut Pending,
) {
    if !super::lens_filters(app.backlog_view.lens) {
        return;
    }
    let scope_total: usize = super::scoped_repos(app, snap)
        .iter()
        .map(|row| row.repo.tasks.len())
        .sum();
    let batch = clear_batch(app, snap);
    let total = batch.total();
    let selecting = !app.backlog_view.bulk_selected_tasks.is_empty();
    let enabled = total > 0 && (selecting || total < scope_total);
    let scope_word = if selecting { "selected" } else { "showing" };

    if app.backlog_view.bulk_archive_confirm {
        // The confirm always spells out the split, even when there is only
        // one disposition — this is the last point before files move, and
        // "Archive 12 · complete 3" is the sentence that catches a selection
        // the user did not mean to make.
        let detail = match (batch.archive_count(), batch.complete_count()) {
            (0, complete) => format!("Complete {complete} Done tasks?"),
            (archive, 0) => format!("Archive {archive} tasks?"),
            (archive, complete) => {
                format!("Archive {archive} · complete {complete} Done?")
            }
        };
        ui.colored_label(theme::amber(), detail);
        if ui.add(theme::danger_button("Confirm")).clicked() {
            app.backlog_status
                .set(format!("{} {total} tasks", batch.progress_verb()));
            pending.bulk_clear = Some(batch);
            app.backlog_view.bulk_archive_confirm = false;
            app.backlog_view.bulk_selected_tasks.clear();
            app.backlog_view.bulk_selection_anchor = None;
        }
        if ui.button("Cancel").clicked() {
            app.backlog_view.bulk_archive_confirm = false;
        }
    } else if ui
        .add_enabled(
            enabled,
            egui::Button::new(format!("{} {total} {scope_word}", batch.verb())),
        )
        .on_hover_text(if enabled {
            "Move these tasks off the active board — Done tasks are completed, the rest archived"
        } else if selecting {
            "Nothing in the selection can be cleared"
        } else {
            "Select cards, or narrow the view with a filter — this clears everything currently shown"
        })
        .clicked()
    {
        app.backlog_view.bulk_archive_confirm = true;
    }
}

/// The filter controls. Like `render_lens_tabs`, renders bare inside the
/// container the caller provides.
pub(super) fn render_project_toolbar(app: &mut HiveApp, ui: &mut egui::Ui, snap: &Snapshot) {
    {
        let active_count = usize::from(!app.filter().is_empty())
            + usize::from(!app.backlog_view.repo_filter.is_empty())
            + usize::from(app.backlog_view.selected_repo.is_some())
            + usize::from(app.backlog_view.status_filter != "all")
            + usize::from(app.backlog_view.priority_filter != "all")
            + usize::from(app.backlog_view.project_filter != "all")
            + usize::from(app.backlog_view.label_filter != "all")
            + usize::from(app.backlog_view.show_completed)
            + usize::from(app.backlog_view.show_archived)
            + usize::from(!app.backlog_view.show_drafts)
            + usize::from(app.backlog_view.stale_only);
        let compact = ui.available_width() < 640.0;
        let repo_filter_width = if compact { 140.0 } else { 180.0 };
        let project_picker_width = if compact { 160.0 } else { 280.0 };
        ui.horizontal_wrapped(|ui| {
            ui.label(egui::RichText::new("Filters").strong());
            if active_count > 0 {
                ui.label(
                    egui::RichText::new(format!("{active_count} active"))
                        .small()
                        .color(theme::lavender()),
                );
            }
            ui.separator();
            crate::ui::filter_bar::facet_label(ui, "Repo");
            ui.add(
                egui::TextEdit::singleline(&mut app.backlog_view.repo_filter)
                    .id_salt("backlog_repo_filter")
                    .hint_text("Filter repos")
                    .desired_width(repo_filter_width),
            );
            let repo_filter_lc = app.backlog_view.repo_filter.to_lowercase();
            let combo_label = app
                .backlog_view
                .selected_repo
                .as_deref()
                .and_then(|key| snap.repo(key))
                .map(|row| row.label())
                .unwrap_or_else(|| "All repos".to_string());
            egui::ComboBox::from_id_salt("backlog_project_picker")
                .selected_text(combo_label)
                .width(project_picker_width)
                .show_ui(ui, |ui| {
                    let mut shown = 0usize;
                    if repo_filter_lc.is_empty() || "all repos".contains(&repo_filter_lc) {
                        shown += 1;
                        let selected = app.backlog_view.selected_repo.is_none();
                        let total_open: usize = snap
                            .repos
                            .iter()
                            .map(|row| sort::open_task_count(&row.repo))
                            .sum();
                        if ui
                            .selectable_label(
                                selected,
                                format!("All repos  ·  {total_open} open"),
                            )
                            .clicked()
                        {
                            app.backlog_view.selected_repo = None;
                            reset_task_selection(app);
                        }
                    }
                    for row in &snap.repos {
                        if !row.matches_filter(&repo_filter_lc) {
                            continue;
                        }
                        shown += 1;
                        let selected =
                            app.backlog_view.selected_repo.as_deref() == Some(&row.key);
                        let label = format!(
                            "{}  ·  {} open",
                            row.label(),
                            sort::open_task_count(&row.repo)
                        );
                        if ui.selectable_label(selected, label).clicked() {
                            app.backlog_view.selected_repo = Some(row.key.clone());
                            reset_task_selection(app);
                        }
                    }
                    if shown == 0 {
                        ui.label(
                            egui::RichText::new("No matching repos").color(theme::muted_text()),
                        );
                    }
                });

            if compact {
                ui.end_row();
            }

            ui.separator();
            crate::ui::filter_bar::facet_label(ui, "Status");
            // Owner UX pass (2026-08-05): the same shared vocabulary Board's
            // columns, the detail-pane editor, and Statistics all consume now,
            // so this dropdown can no longer offer a different status set than
            // what Board actually shows (previously this used a local union
            // that omitted a repo's declared-but-currently-empty statuses).
            let scoped = super::scoped_repos(app, snap);
            let statuses = ordered_status_vocabulary(scoped.iter().map(|row| &row.repo));
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

            crate::ui::filter_bar::facet_label(ui, "Priority");
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

            crate::ui::filter_bar::facet_label(ui, "Project");
            // Both option lists are built here, before any combo can
            // mutate `app`: they borrow it immutably via `ActiveFilters`,
            // and each `selectable_value` below needs it mutably.
            let (milestones, labels) = {
                let facet_filter_lc = app.filter().to_lowercase();
                let filters = sort::ActiveFilters::from_app(app, &facet_filter_lc);
                let scoped = super::scoped_repos(app, snap);
                (
                    sort::project_options(&scoped, &filters, &app.backlog_view.project_filter),
                    sort::label_options(&scoped, &filters, &app.backlog_view.label_filter),
                )
            };
            egui::ComboBox::from_id_salt("backlog_project_filter")
                .selected_text(format::value_filter_label(
                    &app.backlog_view.project_filter,
                ))
                .show_ui(ui, |ui| {
                    ui.selectable_value(
                        &mut app.backlog_view.project_filter,
                        "all".to_string(),
                        "All",
                    );
                    for option in milestones {
                        let label = format!("{}  ({})", option.value, option.count);
                        ui.selectable_value(
                            &mut app.backlog_view.project_filter,
                            option.value,
                            label,
                        );
                    }
                });

            crate::ui::filter_bar::facet_label(ui, "Label");
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
            if crate::ui::filter_bar::clear(ui, active_count > 0) {
                app.filter_mut().clear();
                app.backlog_view.repo_filter.clear();
                app.backlog_view.selected_repo = None;
                app.backlog_view.status_filter = "all".to_string();
                app.backlog_view.priority_filter = "all".to_string();
                app.backlog_view.project_filter = "all".to_string();
                app.backlog_view.label_filter = "all".to_string();
                app.backlog_view.show_completed = false;
                app.backlog_view.show_archived = false;
                app.backlog_view.show_drafts = true;
                app.backlog_view.stale_only = false;
                app.backlog_view.bulk_archive_confirm = false;
                reset_task_selection(app);
            }
        });
    }
}
