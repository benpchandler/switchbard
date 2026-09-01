//! The merged table's row rendering — mock §6's six columns: `Worktree /
//! Git / Services / Listening / Agent / actions`. One virtualized
//! `egui_extras::TableBuilder` row per worktree (primary or linked), plus
//! one per external squatter listener at the bottom.
//!
//! Every action a cell renders queues into `Pending` (never mutates `app`
//! directly except the bulk-select checkbox and the perf counter, both of
//! which are cheap, synchronous, and side-effect-free enough to not need
//! queuing) — see `super`'s module doc for why.

use std::path::Path;

use eframe::egui;

use crate::app::HiveApp;
use crate::runtime::worktree_names::worktree_display_name;
use crate::runtime::{removal_facts, WorktreeMeta};
use crate::ui::components::table_shell;
use crate::ui::components::{branch_label, mono_label, path_cell, status_pill, StatusKind};
use crate::ui::theme;
use switchbard_core::{
    AttributedListener, RemovalIntent, RemovalSafety, RemovalVerdict, Repo, ResolvedService,
    WorktreeRef,
};

use super::{agent, bulk_remove, chips, git_chip, OpsRow, Pending, Snapshot};

/// The most service chips a busy row shows inline before collapsing the rest
/// behind a "+N" overflow chip (TASK-100 medic pass). `horizontal_wrapped`'s
/// multi-line growth used to paint straight through the fixed `ROW_HEIGHT`
/// into the row below it once a worktree ran enough services — see the
/// Services/Listening cells below and their `.clip(true)` columns, the
/// defense-in-depth backstop for whatever a cap doesn't catch.
const MAX_VISIBLE_SERVICE_CHIPS: usize = 3;

/// The Listening cell's own cap, smaller than [`MAX_VISIBLE_SERVICE_CHIPS`]:
/// a listener chip (dot + `:port` + open + kill, four widgets, ~100px at
/// this column's width) is much wider than a service chip (icon + name), so
/// the same count doesn't fit the same column width — a fixture with 3 busy
/// listeners clipped chips mid-label at a cap of either 3 or 2, measured
/// directly off the `ops_table_busy_row` screenshot rather than guessed.
const MAX_VISIBLE_LISTENER_CHIPS: usize = 1;

const ROW_HEIGHT: f32 = 26.0;

pub(super) fn render_table(
    ui: &mut egui::Ui,
    app: &mut HiveApp,
    snap: &Snapshot,
    pending: &mut Pending,
) {
    // `::both()`, not `::vertical()`: at narrow window widths (mock §7d
    // stress state) the six fixed-plus-remainder columns don't all fit, and
    // a horizontal scrollbar is what keeps the trailing columns (Listening/
    // Agent/actions) reachable instead of silently clipped off the visible
    // window with no way back to them.
    egui::ScrollArea::both()
        .id_salt("ops_table_scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            table_shell(ui, "ops_table")
                .column(egui_extras::Column::initial(200.0).at_least(140.0))
                .column(egui_extras::Column::initial(130.0).at_least(100.0))
                // Services/Listening are the two columns whose content count
                // varies per row (0..N service or listener chips) and, unlike
                // every other column here, used to lay that content out with
                // `horizontal_wrapped` — a row busy enough to wrap grew taller
                // than the table's fixed `ROW_HEIGHT` and painted straight
                // through the row below it (2026-09-01 screenshot review).
                // `render_capped_chip_row` below now caps what renders inline
                // instead of wrapping; `.clip(true)` here is the defense-in-
                // depth backstop for anything that still overflows (e.g. one
                // long chip label).
                .column(egui_extras::Column::remainder().at_least(140.0).clip(true))
                .column(
                    egui_extras::Column::initial(160.0)
                        .at_least(110.0)
                        .clip(true),
                )
                .column(egui_extras::Column::initial(140.0).at_least(90.0))
                // Actions is the one column whose content (Logs + Rename +
                // trash icon + bulk-select checkbox, up to four widgets) must
                // never lose a pixel to squeeze — see the 2026-09-01
                // screenshot review that caught it clipped under the old
                // "everything-fixed, last-column-absorbs-the-rest" layout.
                // Fixed-and-floor-equal, not `remainder()`, so it never
                // shrinks below what its own content needs; Services is the
                // column that gives up space instead, since its chip count
                // varies per row anyway.
                .column(egui_extras::Column::initial(190.0).at_least(190.0))
                .header(22.0, |mut header| {
                    for label in ["Worktree", "Git", "Services", "Listening", "Agent", ""] {
                        header.col(|ui| {
                            ui.label(egui::RichText::new(label).strong().small());
                        });
                    }
                })
                .body(|body| {
                    body.rows(ROW_HEIGHT, snap.rows.len(), |mut table_row| {
                        let row_index = table_row.index();
                        match &snap.rows[row_index] {
                            OpsRow::Worktree {
                                worktree_idx,
                                is_primary,
                            } => {
                                let w = &snap.worktrees[*worktree_idx];
                                let repo = snap.repos.iter().find(|r| r.name == w.repo_name);
                                render_worktree_row(
                                    &mut table_row,
                                    app,
                                    snap,
                                    pending,
                                    w,
                                    repo,
                                    *is_primary,
                                );
                            }
                            OpsRow::Squatter { listener_idx } => {
                                render_squatter_row(
                                    &mut table_row,
                                    &snap.unattributed[*listener_idx],
                                    pending,
                                );
                            }
                        }
                    });
                });
        });
}

fn render_worktree_row(
    table_row: &mut egui_extras::TableRow<'_, '_>,
    app: &mut HiveApp,
    snap: &Snapshot,
    pending: &mut Pending,
    w: &WorktreeRef,
    repo: Option<&Repo>,
    is_primary: bool,
) {
    let default_meta;
    let m = if let Some(meta) = snap.meta.get(&w.path) {
        meta
    } else {
        default_meta = WorktreeMeta::default();
        &default_meta
    };
    let listeners: &[AttributedListener] = snap
        .listeners_by_wt
        .get(&w.path)
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    let svcs: &[ResolvedService] = snap.services.get(&w.path).map(Vec::as_slice).unwrap_or(&[]);
    let selected = app.bulk_selected_worktrees.contains(&w.path);

    // Worktree cell.
    table_row.col(|ui| {
        render_worktree_cell(ui, app, snap, repo, w, m, is_primary);
    });

    // Git cell.
    table_row.col(|ui| {
        render_git_cell(ui, snap, w, m);
    });

    // Services cell.
    table_row.col(|ui| {
        ui.horizontal(|ui| {
            let visible: Vec<&ResolvedService> = svcs
                .iter()
                .filter(|s| !chips::should_skip_service(s, w, snap, app.show_non_servers))
                .filter(|s| chips::service_matches_filter(s, w, &snap.filter_lc))
                .collect();
            render_capped_chip_row(
                ui,
                &visible,
                MAX_VISIBLE_SERVICE_CHIPS,
                |ui, resolved| chips::render_service_chip(ui, w, resolved, snap, pending),
                |hidden| {
                    hidden
                        .iter()
                        .map(|s| s.canonical_name.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                },
            );
        });
    });

    // Listening cell.
    table_row.col(|ui| {
        ui.horizontal(|ui| {
            let service_ports: std::collections::HashSet<u16> =
                svcs.iter().filter_map(|s| s.expected_port).collect();
            let visible: Vec<&AttributedListener> = listeners
                .iter()
                .filter(|l| !service_ports.contains(&l.listener.port))
                .filter(|l| chips::listener_matches(l, &snap.filter_lc))
                .collect();
            render_capped_chip_row(
                ui,
                &visible,
                MAX_VISIBLE_LISTENER_CHIPS,
                |ui, l| chips::render_listening_chip(ui, l, pending),
                |hidden| {
                    hidden
                        .iter()
                        .map(|l| format!(":{}", l.listener.port))
                        .collect::<Vec<_>>()
                        .join(", ")
                },
            );
        });
    });

    // Agent cell.
    table_row.col(|ui| {
        render_agent_cell(ui, snap, &w.path);
    });

    // Actions cell.
    table_row.col(|ui| {
        render_actions_cell(ui, app, snap, repo, w, is_primary, listeners.len(), pending);
    });

    if selected {
        paint_selection_ring(table_row);
    }

    let expanded = true; // every row shows everything now — no collapse state.
    app.perf_count_worktree_row(expanded, svcs.len(), listeners.len());
}

/// Row selection uses the stroke-ring convention (mock's implementation
/// obligations, `docs/product-trajectory.md`'s IA V2 entry) — a dedicated
/// `theme::selected_row_stroke()` border, not `egui_extras`'s built-in
/// `TableRow::set_selected` (which paints `ui.visuals().selection`, a
/// different, unbranded color). Painted via `response.ctx`'s always-available
/// painter rather than the `Ui` used to build any one cell, because by the
/// time every column has been added there is no single still-borrowable `Ui`
/// left that spans the whole row — only the union `Response` `TableRow::
/// response()` hands back.
fn paint_selection_ring(table_row: &egui_extras::TableRow<'_, '_>) {
    let resp = table_row.response();
    resp.ctx.debug_painter().rect_stroke(
        resp.rect,
        2.0,
        theme::selected_row_stroke(),
        egui::StrokeKind::Inside,
    );
}

#[allow(clippy::too_many_arguments)]
fn render_worktree_cell(
    ui: &mut egui::Ui,
    app: &HiveApp,
    snap: &Snapshot,
    repo: Option<&Repo>,
    w: &WorktreeRef,
    m: &WorktreeMeta,
    is_primary: bool,
) {
    ui.horizontal(|ui| {
        if is_primary {
            if let Some(repo) = repo {
                ui.label(egui::RichText::new(&repo.name).strong());
                ui.label(egui::RichText::new("·").color(theme::weak_text()));
                branch_label(ui, w.branch.as_deref());
            } else {
                // No repo match (shouldn't happen — rows are built from
                // `snap.repos` — but never panic the render path over it).
                let display_name = worktree_display_name(&app.config, &fallback_repo(w), w);
                ui.label(egui::RichText::new(display_name).strong());
            }
        } else {
            theme::painted_hook_arrow(ui, theme::weak_text());
            branch_label(ui, w.branch.as_deref());
        }
        // feat/landing-stage (TASK-41): PR-in-flight status for a worktree
        // holding unlanded work. Lives beside the identity it describes now
        // — the retired swimlane row put it in a right-aligned trailing
        // cluster, which this table's Actions column repurposes for verbs
        // only.
        let landing_view = super::landing::landing_chip(
            super::has_unlanded_work(&m.trunk),
            w.branch.is_some(),
            snap.landing.lock().unwrap().get(&w.path),
        );
        super::landing::render_landing_chip(ui, landing_view);
    });
}

/// Only reached when a worktree's row has no matching `Repo` in the current
/// scope — defensive, not expected in practice (`compute_rows` only builds
/// worktree rows by walking `snap.repos`).
fn fallback_repo(w: &WorktreeRef) -> Repo {
    Repo {
        name: w.repo_name.clone(),
        path: w.path.clone(),
    }
}

fn render_git_cell(ui: &mut egui::Ui, snap: &Snapshot, w: &WorktreeRef, m: &WorktreeMeta) {
    // TASK-100 medic pass: one compact chip (mock §6 — "dirty · ahead 1",
    // "ahead 2", "clean"), not five always-visible fragments; see
    // `git_chip`'s module doc for the screenshot review that flagged the
    // old layout. `snap.sizes` is still read here — TASK-41's
    // independently-cadenced on-disk-size cache (`workers::spawn_size`) —
    // it just folds into the chip's hover instead of its own label now.
    let chip = git_chip::compute_git_chip(m, snap.sizes.get(&w.path));
    git_chip::render_git_chip(ui, &chip);
}

/// Renders up to `max_visible` items in a single, non-wrapping horizontal
/// line, plus a "+N" overflow chip (hover-detailed via `overflow_detail`)
/// when there are more. Replaces `horizontal_wrapped` in the Services/
/// Listening cells — see [`MAX_VISIBLE_SERVICE_CHIPS`]'s doc for why: a
/// capped, single-line list that admits what's hidden is both narrower and
/// more honest than silently wrapping into the row below. `max_visible` is a
/// per-cell parameter rather than one shared constant because a listener
/// chip and a service chip aren't the same width — see
/// [`MAX_VISIBLE_LISTENER_CHIPS`]'s doc.
fn render_capped_chip_row<T>(
    ui: &mut egui::Ui,
    items: &[T],
    max_visible: usize,
    mut render_item: impl FnMut(&mut egui::Ui, &T),
    overflow_detail: impl FnOnce(&[T]) -> String,
) {
    if items.is_empty() {
        ui.label(egui::RichText::new("—").color(theme::weak_text()));
        return;
    }
    let visible_count = items.len().min(max_visible);
    for item in &items[..visible_count] {
        render_item(ui, item);
    }
    if items.len() > visible_count {
        let hidden = &items[visible_count..];
        let label = format!("+{}", hidden.len());
        ui.label(egui::RichText::new(label).small().color(theme::weak_text()))
            .on_hover_text(overflow_detail(hidden));
    }
}

/// The Agent cell: dispatch-run attribution today; see `agent`'s module doc
/// for the TASK-98 interactive-session seam this leaves.
fn render_agent_cell(ui: &mut egui::Ui, snap: &Snapshot, wt_path: &Path) {
    match snap.agent_attribution_by_wt.get(wt_path) {
        Some(attribution) => {
            status_pill(
                ui,
                StatusKind::Info,
                agent::label(attribution),
                Some("Headless dispatch run(s) currently holding this worktree"),
            );
        }
        None => {
            ui.label(egui::RichText::new("—").color(theme::weak_text()));
        }
    }
}

/// TASK-100: the "forthcoming Open log action" `ActiveRun::log_path`'s doc
/// comment named — first surfaced here as the Actions cell's Logs icon.
/// Shown only when a switchbard-started run is live for this worktree; picks
/// the first such run when more than one service is running (rare, and every
/// run's log still opens on the row's next redraw once the others finish).
///
/// TASK-100 medic pass: right-to-left layout, not left-to-right. The Logs
/// icon only renders for a row with a live switchbard-started run, so under
/// the old left-to-right layout every widget after it shifted depending on
/// whether that one optional icon was present — Rename's on-screen position
/// "drifted" row to row, which the 2026-09-01 screenshot review flagged.
/// Anchoring the cluster to the column's right edge instead means Trash and
/// the bulk-select checkbox sit in the exact same place on every non-primary
/// row regardless of Logs, and Rename does too once "+ New worktree" (primary
/// rows only) is accounted for.
#[allow(clippy::too_many_arguments)]
fn render_actions_cell(
    ui: &mut egui::Ui,
    app: &mut HiveApp,
    snap: &Snapshot,
    repo: Option<&Repo>,
    w: &WorktreeRef,
    is_primary: bool,
    listener_count: usize,
    pending: &mut Pending,
) {
    let running_log = snap
        .active_runs
        .values()
        .find(|r| r.worktree_path == w.path)
        .map(|r| r.log_path.clone());

    // Added in reverse of the intended left-to-right reading order — under
    // `Layout::right_to_left`, the first widget added lands at the column's
    // right edge and each later one is placed to its left.
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        if !is_primary {
            bulk_remove::render_select_checkbox(
                ui,
                w,
                is_primary,
                &mut app.bulk_selected_worktrees,
            );
            ui.add_space(2.0);
            let m = snap.meta.get(&w.path).cloned().unwrap_or_default();
            let facts = removal_facts(
                is_primary,
                &m,
                super::attached_processes(snap, &w.path, listener_count),
            );
            let verdict =
                RemovalSafety::evaluate(&facts, RemovalIntent::WorktreeAndBranch).verdict();
            // `Blocked` and `Safe` used to be the only two colors this base
            // could take (`_ => weak_text()` swallowed everything else,
            // including `Checking`) — a probe still in flight at rest looked
            // exactly as settled as a verdict that had actually run and
            // passed. `Primary` never reaches this branch (`!is_primary`
            // guards it) but is matched explicitly rather than folded into a
            // wildcard, so a future `RemovalVerdict` variant fails to
            // compile here instead of silently inheriting `weak_text()`.
            let base = match verdict {
                RemovalVerdict::Blocked => theme::amber(),
                RemovalVerdict::Checking => theme::scale_alpha(theme::amber(), 0.55),
                RemovalVerdict::Safe | RemovalVerdict::Primary => theme::weak_text(),
            };
            let resp = theme::painted_trash_button(ui, base);
            if resp.on_hover_text("Remove worktree…").clicked() {
                pending.open_remove_worktree =
                    Some((w.repo_name.clone(), w.path.clone(), w.branch.clone()));
            }
        }
        if is_primary {
            if let Some(repo) = repo {
                if ui
                    .small_button("+ New worktree")
                    .on_hover_text(format!("Create a new worktree for '{}'", repo.name))
                    .clicked()
                {
                    pending.open_create_worktree = Some(repo.clone());
                }
            }
        }
        if let Some(repo) = repo {
            let resp = theme::painted_rename_button(ui, theme::weak_text());
            if resp.on_hover_text("Rename Switchbard label").clicked() {
                pending.open_rename_worktree = Some((repo.clone(), w.clone()));
            }
        }
        if let Some(log_path) = running_log {
            if theme::painted_logs_button(ui, theme::weak_text())
                .on_hover_text(format!("View logs — {}", log_path.display()))
                .clicked()
            {
                app.open_log_file(&log_path);
            }
        }
    });
}

fn render_squatter_row(
    table_row: &mut egui_extras::TableRow<'_, '_>,
    l: &AttributedListener,
    pending: &mut Pending,
) {
    table_row.col(|ui| {
        ui.label(
            egui::RichText::new(format!("external process · pid {}", l.listener.pid))
                .color(theme::weak_text()),
        );
    });
    table_row.col(|ui| {
        ui.label(egui::RichText::new("—").color(theme::weak_text()));
    });
    table_row.col(|ui| {
        ui.label(egui::RichText::new("—").color(theme::weak_text()));
    });
    table_row.col(|ui| {
        ui.horizontal(|ui| {
            theme::painted_dot(ui, theme::amber());
            mono_label(ui, &format!(":{}", l.listener.port), None)
                .on_hover_text("Not owned by any tracked worktree");
            ui.label(&l.listener.command_name);
            if let Some(cwd) = &l.listener.cwd {
                path_cell(ui, cwd);
            }
        });
    });
    table_row.col(|ui| {
        ui.label(egui::RichText::new("—").color(theme::weak_text()));
    });
    table_row.col(|ui| {
        if theme::painted_kill_button(ui)
            .on_hover_text(format!(
                "Kill pid {} ({})",
                l.listener.pid, l.listener.command_name
            ))
            .clicked()
        {
            pending.kill = Some(l.listener.pgid);
        }
    });
}
