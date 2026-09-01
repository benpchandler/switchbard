//! TASK-97: the Tasks place's List body — flattens the current frame's
//! groups (or, ungrouped, the whole visible set as one implicit group) into
//! a single uniform-height row list and renders it through `egui::
//! ScrollArea::show_rows` (TASK-13's virtualization pattern: only rows
//! scrolled into view get their widgets built each frame — no unbounded
//! per-frame allocation as the task count grows).
//!
//! Every entry in the flattened list — a group header, its expanded
//! summary band (at most one extra row per expanded group), or a task row
//! at whatever sub-issue depth — is exactly [`ROW_HEIGHT`] tall. That
//! uniformity is what makes `show_rows`' viewport math correct; see the
//! module's own row-height note below for why the summary band fits in one
//! slot rather than needing its own variable-height carve-out.

use std::collections::HashSet;

use eframe::egui;

use crate::app::HiveApp;
use crate::runtime::BacklogTaskKey;
use crate::ui::backlog::{list, Pending, TaskRow};
use crate::ui::theme;

use super::groups::Group;

/// Every flat-list row — headers, an expanded group's one-row summary band,
/// and task rows — renders at this height. Tall enough for the summary
/// band's horizontal strip of meter + counts + goal chip + description
/// (mock §3 renders that whole band as a single row), short enough to stay
/// close to the existing List lens's own row height (26–30px).
const ROW_HEIGHT: f32 = 34.0;

/// Sub-issue chains nest one decimal level in practice; bounded the same as
/// `switchbard_core::backlog::ranking::MAX_PARENT_HOPS` so a data cycle
/// can't recurse forever.
const MAX_DEPTH: u8 = 8;

enum FlatRow<'a> {
    Header(usize),
    Summary(usize),
    Task {
        row: TaskRow<'a>,
        depth: u8,
        /// This row's own direct children (within its group — see
        /// `child_keys_in_group`'s doc), passed straight through to
        /// `list::render_task_list_row`'s `children` parameter for the
        /// "[done/total]" title suffix. Empty for a childless row or a
        /// nested child itself (only a parent's own row carries this).
        children: Vec<TaskRow<'a>>,
    },
}

/// Flatten `groups` into the row list `render` virtualizes over. `expanded`
/// is `TasksPlaceState::expanded_groups`; when `groups.len() == 1` and its
/// key is empty (the ungrouped sentinel `render` passes), no header/summary
/// rows are emitted at all — a flat list, same as grouping "None".
///
/// Child detection for "is this row its own top-level bucket entry, or does
/// it nest under a parent" is computed **globally**, across every visible
/// row in every group — not scoped to one group's own members. A child
/// whose own field value would naturally bucket it into a *different* group
/// than its parent (e.g. grouped by Status, parent "In Progress", child
/// "Done") still nests under its parent wherever the parent's group is,
/// never duplicating as a second top-level entry elsewhere — the same
/// "a child belongs with its parent first" rule `tree::child_keys_in_view`
/// already applies to the (ungrouped) legacy List lens, generalized past a
/// single flat list to however many buckets group-by produces.
fn flatten<'a>(
    groups: &'a [Group<'a>],
    expanded: &std::collections::BTreeSet<String>,
    show_headers: bool,
) -> Vec<FlatRow<'a>> {
    let all_visible: Vec<TaskRow<'a>> =
        groups.iter().flat_map(|g| g.rows.iter().copied()).collect();
    let child_keys = child_keys_in_view(&all_visible);

    let mut flat = Vec::new();
    for (group_index, group) in groups.iter().enumerate() {
        if show_headers {
            flat.push(FlatRow::Header(group_index));
            if expanded.contains(&group.key) {
                flat.push(FlatRow::Summary(group_index));
            }
        }
        for row in &group.rows {
            if child_keys.contains(&row.key()) {
                continue; // rendered nested under its parent below, wherever that parent's group is
            }
            push_row_and_children(&mut flat, *row, 0);
        }
    }
    flat
}

/// Keys of every row in `visible` whose `parent` names another row also
/// present in `visible` — the same predicate `tree::child_keys_in_view`
/// uses for the legacy (ungrouped) List lens, applied here across every
/// group's rows at once rather than one flat list.
fn child_keys_in_view(visible: &[TaskRow<'_>]) -> HashSet<BacklogTaskKey> {
    visible
        .iter()
        .filter(|row| {
            row.task.parent.as_ref().is_some_and(|parent_id| {
                visible
                    .iter()
                    .any(|other| other.repo.key == row.repo.key && &other.task.id == parent_id)
            })
        })
        .map(TaskRow::key)
        .collect()
}

/// Push `row`, then recurse into its *real* children (`switchbard_core::
/// children` against the full repo, not the filtered/visible set) —
/// deliberately the same "expanding a parent should reveal its whole
/// sub-tree, not silently hide children the status/priority filters happen
/// to exclude" rule `tree.rs`'s own module doc states for the legacy lens's
/// `expanded_parents` toggle. TASK-97's sub-issues are always expanded
/// (decision record Q9 = A), so that rule now applies unconditionally: a
/// Done child stays reachable — nested under its parent, with an honest
/// roll-up badge — even while the "Done" visibility toggle hides it (and
/// every other Done task) from the top-level list.
fn push_row_and_children<'a>(flat: &mut Vec<FlatRow<'a>>, row: TaskRow<'a>, depth: u8) {
    let kids: Vec<TaskRow<'a>> = switchbard_core::children(row.task, &row.repo.repo)
        .into_iter()
        .map(|task| TaskRow {
            repo: row.repo,
            task,
        })
        .collect();
    flat.push(FlatRow::Task {
        row,
        depth,
        children: kids.clone(),
    });
    if depth < MAX_DEPTH {
        for kid in kids {
            push_row_and_children(flat, kid, depth + 1);
        }
    }
}

/// Render the List body: column header, then the virtualized flat list.
/// `groups` is `groups::build_groups`'s output when grouped, or a single
/// synthetic group holding every visible row (empty key, header suppressed)
/// when `group_by` is `None` — `render` (mod.rs) makes that call.
pub(super) fn render(
    app: &mut HiveApp,
    ui: &mut egui::Ui,
    groups: &[Group<'_>],
    show_headers: bool,
    pending: &mut Pending,
) {
    let all_visible: Vec<TaskRow<'_>> =
        groups.iter().flat_map(|g| g.rows.iter().copied()).collect();
    let visible_keys: Vec<BacklogTaskKey> = all_visible.iter().map(TaskRow::key).collect();
    let flat = flatten(groups, &app.tasks_place.expanded_groups, show_headers);

    render_column_header(app, ui, &all_visible);

    if flat.is_empty() {
        ui.add_space(20.0);
        ui.label(egui::RichText::new("No tasks match the current filters").strong());
        return;
    }

    egui::ScrollArea::vertical()
        .id_salt("tasks_place_list_body")
        .auto_shrink([false, false])
        .show_rows(ui, ROW_HEIGHT, flat.len(), |ui, row_range| {
            for index in row_range {
                match &flat[index] {
                    FlatRow::Header(group_index) => {
                        super::header::render(app, ui, groups, *group_index, ROW_HEIGHT);
                    }
                    FlatRow::Summary(group_index) => {
                        super::header::render_summary(ui, &groups[*group_index], ROW_HEIGHT);
                    }
                    FlatRow::Task {
                        row,
                        depth,
                        children,
                    } => {
                        let child_tasks: Vec<&switchbard_core::BacklogTask> =
                            children.iter().map(|kid| kid.task).collect();
                        ui.allocate_ui(egui::vec2(ui.available_width(), ROW_HEIGHT), |ui| {
                            // TASK-97 directive #9: repo badges on rows,
                            // always — `show_repo: true` unconditionally
                            // (the Tasks place has no single-repo picker to
                            // key off, unlike the legacy lens's `backlog_
                            // view.selected_repo`). `always_expanded: true`
                            // — sub-issues are always expanded, no per-row
                            // caret (decision record Q9 = A).
                            list::render_task_list_row(
                                app,
                                ui,
                                row,
                                &all_visible,
                                &visible_keys,
                                true,
                                *depth as usize,
                                &child_tasks,
                                true,
                                pending,
                            );
                        });
                    }
                }
            }
        });
}

/// Select-all checkbox + column labels — reuses `list::
/// render_select_all_checkbox` (toggles every *visible* row's bulk
/// selection, same as the legacy List lens) rather than a fresh
/// implementation, so the two never define "select all" differently.
fn render_column_header(app: &mut HiveApp, ui: &mut egui::Ui, all_visible: &[TaskRow<'_>]) {
    ui.horizontal(|ui| {
        list::render_select_all_checkbox(app, ui, all_visible);
        ui.add_sized(
            [ui.available_width() - 236.0, 18.0],
            egui::Label::new(
                egui::RichText::new("Task")
                    .small()
                    .color(theme::muted_text()),
            ),
        );
        ui.add_sized(
            [92.0, 18.0],
            egui::Label::new(
                egui::RichText::new("Repo")
                    .small()
                    .color(theme::muted_text()),
            ),
        );
        ui.add_sized(
            [86.0, 18.0],
            egui::Label::new(
                egui::RichText::new("Status")
                    .small()
                    .color(theme::muted_text()),
            ),
        );
        ui.add_sized(
            [62.0, 18.0],
            egui::Label::new(
                egui::RichText::new("Priority")
                    .small()
                    .color(theme::muted_text()),
            ),
        );
    });
    ui.separator();
}
