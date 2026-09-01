//! The merged table's row rendering — mock §6's six columns: `Worktree /
//! Git / Services / Listening / Agent / actions`. One virtualized
//! `egui_extras::TableBuilder` row per worktree (primary or linked), plus
//! one per external squatter listener at the bottom.
//!
//! Every action a cell renders queues into `Pending` (never mutates `app`
//! directly except the bulk-select checkbox and the perf counter, both of
//! which are cheap, synchronous, and side-effect-free enough to not need
//! queuing) — see `super`'s module doc for why.

use std::collections::BTreeSet;
use std::path::Path;
use std::time::Instant;

use eframe::egui;

use crate::app::HiveApp;
use crate::runtime::worktree_names::worktree_display_name;
use crate::runtime::{removal_facts, RowState, WorktreeMeta};
use crate::ui::components::table_shell;
use crate::ui::components::{branch_label, mono_label, path_cell, status_pill, StatusKind};
use crate::ui::theme;
use switchbard_core::{
    default_port_for_service, AttributedListener, DriftProbe, RemovalIntent, RemovalSafety,
    RemovalVerdict, Repo, ResolvedService, ServerLikelihood, TrunkDivergence, WorktreeRef,
};

use super::{agent, bulk_remove, is_containerized, staleness, OpsRow, Pending, Snapshot};

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
                .column(egui_extras::Column::remainder().at_least(140.0))
                .column(egui_extras::Column::initial(160.0).at_least(110.0))
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
        ui.horizontal_wrapped(|ui| {
            let visible: Vec<&ResolvedService> = svcs
                .iter()
                .filter(|s| !should_skip_service(s, w, snap, app.show_non_servers))
                .filter(|s| service_matches_filter(s, w, &snap.filter_lc))
                .collect();
            if visible.is_empty() {
                ui.label(egui::RichText::new("—").color(theme::weak_text()));
            }
            for resolved in visible {
                render_service_chip(ui, w, resolved, snap, pending);
            }
        });
    });

    // Listening cell.
    table_row.col(|ui| {
        ui.horizontal_wrapped(|ui| {
            let service_ports: std::collections::HashSet<u16> =
                svcs.iter().filter_map(|s| s.expected_port).collect();
            let visible: Vec<&AttributedListener> = listeners
                .iter()
                .filter(|l| !service_ports.contains(&l.listener.port))
                .filter(|l| listener_matches(l, &snap.filter_lc))
                .collect();
            if visible.is_empty() {
                ui.label(egui::RichText::new("—").color(theme::weak_text()));
            }
            for l in visible {
                render_listening_chip(ui, l, pending);
            }
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
    ui.horizontal(|ui| {
        render_dirty_chip(ui, m);
        render_trunk_chip(ui, m.trunk.as_ref());
        render_drift_chip(ui, m.remote_drift.as_ref());
        staleness::render_staleness_badge(ui, m);
        // TASK-41: on-disk size, same cadence-independent cache the retired
        // swimlane row read (`workers::spawn_size`) — see `Snapshot::sizes`'s
        // doc for why it can't share the git-probe tick.
        staleness::render_size_label(ui, snap.sizes.get(&w.path));
    });
}

fn render_dirty_chip(ui: &mut egui::Ui, m: &WorktreeMeta) {
    match m.is_dirty() {
        Some(true) => status_pill(ui, StatusKind::Warn, "dirty", Some("Uncommitted changes")),
        Some(false) => status_pill(
            ui,
            StatusKind::Neutral,
            "clean",
            Some("No uncommitted changes"),
        ),
        None => ui
            .label(egui::RichText::new("…").color(theme::weak_text()))
            .on_hover_text("Dirty probe pending or failed"),
    };
}

fn render_trunk_chip(ui: &mut egui::Ui, divergence: Option<&TrunkDivergence>) {
    let Some(d) = divergence else {
        return;
    };
    if d.unlanded + d.behind == 0 {
        return;
    }
    let text = format!("+{}/-{}", d.unlanded, d.behind);
    mono_label(ui, &text, Some(theme::lavender())).on_hover_text(format!(
        "{} unlanded commit(s) vs {}, {} behind",
        d.unlanded, d.base, d.behind
    ));
}

fn render_drift_chip(ui: &mut egui::Ui, probe: Option<&DriftProbe>) {
    let Some(probe) = probe else {
        return;
    };
    match probe {
        DriftProbe::Ready { ahead, behind, .. } if ahead + behind > 0 => {
            let text = format!("remote +{ahead}/-{behind}");
            mono_label(ui, &text, Some(theme::sky()))
                .on_hover_text("Divergence from the configured remote upstream");
        }
        DriftProbe::NoUpstream => {
            status_pill(
                ui,
                StatusKind::Warn,
                "no upstream",
                Some("No tracked remote branch"),
            );
        }
        _ => {}
    }
}

fn should_skip_service(
    resolved: &ResolvedService,
    w: &WorktreeRef,
    snap: &Snapshot,
    show_non_servers: bool,
) -> bool {
    if show_non_servers {
        return false;
    }
    if resolved.likelihood != ServerLikelihood::NotServer {
        return false;
    }
    snap.run_for_resolved(&w.path, resolved).is_none()
}

pub(super) fn service_matches_filter(
    resolved: &ResolvedService,
    w: &WorktreeRef,
    filter_lc: &str,
) -> bool {
    if filter_lc.is_empty() {
        return true;
    }
    let hay = format!(
        "{} {} {} {} {}",
        w.repo_name,
        w.branch.as_deref().unwrap_or(""),
        resolved.canonical_name,
        resolved
            .entry_points
            .iter()
            .map(|e| e.name.as_str())
            .collect::<Vec<_>>()
            .join(" "),
        resolved
            .entry_points
            .iter()
            .map(|e| e.command.as_str())
            .collect::<Vec<_>>()
            .join(" "),
    );
    hay.to_lowercase().contains(filter_lc)
}

/// One service's compact icon+name chip — mock: "▶ gui" (idle) or "■ vite"
/// (running). `Blocked`/`ExternalLive` get the same treatment as the retired
/// swimlane strip's per-state action set, just icon-sized.
fn render_service_chip(
    ui: &mut egui::Ui,
    w: &WorktreeRef,
    resolved: &ResolvedService,
    snap: &Snapshot,
    pending: &mut Pending,
) {
    let run = snap.run_for_resolved(&w.path, resolved);
    let containerized = is_containerized(resolved);
    let row_state = RowState::compute(
        resolved.expected_port,
        &w.path,
        run,
        &snap.by_port,
        containerized,
    );
    let primary_ep = resolved.primary_entry_point();
    let name = &resolved.canonical_name;

    ui.horizontal(|ui| match &row_state {
        RowState::Idle => {
            let resp = theme::painted_play_button(ui, theme::weak_text());
            if resp.on_hover_text(format!("Start {name}")).clicked() {
                pending.start = Some((w.path.clone(), primary_ep.clone()));
            }
            ui.label(name);
        }
        RowState::Blocked {
            port, holder_label, ..
        } => {
            let resp = theme::painted_play_button(ui, theme::warn_orange());
            resp.on_hover_text(format!(
                "{name} — blocked: :{port} already held by {holder_label}"
            ));
            ui.label(egui::RichText::new(name).color(theme::weak_text()));
        }
        RowState::Running {
            pgid, started_at, ..
        } => {
            let resp = theme::painted_stop_button(ui, theme::green());
            if resp.on_hover_text(format!("Stop {name}")).clicked() {
                pending.stop = Some((*pgid, primary_ep.name.clone()));
            }
            ui.label(name);
            let hint = open_port_for_running(*pgid, &w.path, resolved, snap);
            let open_resp = theme::painted_open_button(
                ui,
                if hint.is_some() {
                    theme::sky()
                } else {
                    theme::weak_text()
                },
            );
            let open_resp = match &hint {
                Some(h) => open_resp.on_hover_text(h.tooltip()),
                None => open_resp.on_disabled_hover_text(
                    "no listener observed, no port declared, no default known",
                ),
            };
            if open_resp.clicked() {
                if let Some(h) = hint {
                    pending.open = Some(h.port);
                }
            }
            ui.label(
                egui::RichText::new(format!("· {}", uptime_short(*started_at)))
                    .small()
                    .color(theme::weak_text()),
            );
        }
        RowState::ExternalLive { port, .. } => {
            let open_resp = theme::painted_open_button(ui, theme::sky());
            if open_resp
                .on_hover_text(format!("Open :{port} in browser"))
                .clicked()
            {
                pending.open = Some(*port);
            }
            ui.label(name);
            if let Some(al) = snap.by_port.get(port) {
                let kill_resp = theme::painted_kill_button(ui);
                if kill_resp
                    .on_hover_text(format!(
                        "Kill the external process holding :{port} (pid {} · {})",
                        al.listener.pid, al.listener.command_name
                    ))
                    .clicked()
                {
                    pending.kill = Some(al.listener.pgid);
                }
            }
        }
    });
}

fn uptime_short(started_at: Instant) -> String {
    let s = started_at.elapsed().as_secs();
    if s < 60 {
        format!("{s}s")
    } else if s < 3600 {
        format!("{}m", s / 60)
    } else {
        format!("{}h", s / 3600)
    }
}

/// Tiered Open-button port resolution — unchanged from the retired swimlane
/// view; see the four-tier doc that used to live on this function for the
/// storybook/vite-detach reasoning. Trimmed here to the essential contract.
#[derive(Debug, Clone, PartialEq, Eq)]
struct OpenPortHint {
    port: u16,
}

impl OpenPortHint {
    fn tooltip(&self) -> String {
        format!("Open :{} in browser", self.port)
    }
}

fn open_port_for_running(
    run_pgid: i32,
    worktree_path: &Path,
    resolved: &ResolvedService,
    snap: &Snapshot,
) -> Option<OpenPortHint> {
    if let Some(port) = snap
        .ports_by_pgid
        .get(&run_pgid)
        .and_then(|ports| ports.first().copied())
    {
        return Some(OpenPortHint { port });
    }
    if let Some(port) = unclaimed_worktree_listener_port(run_pgid, worktree_path, snap) {
        return Some(OpenPortHint { port });
    }
    if let Some(port) = resolved.expected_port {
        return Some(OpenPortHint { port });
    }
    if let Some(port) = default_port_for_service(&resolved.canonical_name) {
        return Some(OpenPortHint { port });
    }
    None
}

fn unclaimed_worktree_listener_port(
    run_pgid: i32,
    worktree_path: &Path,
    snap: &Snapshot,
) -> Option<u16> {
    let listeners = snap.listeners_by_wt.get(worktree_path)?;
    let other_run_pgids: BTreeSet<i32> = snap
        .active_runs
        .values()
        .filter(|r| r.worktree_path == worktree_path && r.pgid != run_pgid)
        .map(|r| r.pgid)
        .collect();
    let candidates: Vec<u16> = listeners
        .iter()
        .filter(|al| al.listener.pgid != run_pgid && !other_run_pgids.contains(&al.listener.pgid))
        .map(|al| al.listener.port)
        .collect();
    if candidates.len() == 1 {
        candidates.first().copied()
    } else {
        None
    }
}

fn render_listening_chip(ui: &mut egui::Ui, l: &AttributedListener, pending: &mut Pending) {
    ui.horizontal(|ui| {
        theme::painted_dot(ui, theme::green());
        mono_label(ui, &format!(":{}", l.listener.port), None).on_hover_text(format!(
            "{}\npid {} · pgid {}",
            l.listener.command_name, l.listener.pid, l.listener.pgid
        ));
        if theme::painted_open_button(ui, theme::sky())
            .on_hover_text(format!("Open :{} in browser", l.listener.port))
            .clicked()
        {
            pending.open = Some(l.listener.port);
        }
        if theme::painted_kill_button(ui)
            .on_hover_text(format!(
                "Kill {} (pid {})",
                l.listener.command_name, l.listener.pid
            ))
            .clicked()
        {
            pending.kill = Some(l.listener.pgid);
        }
    });
}

pub(super) fn listener_matches(l: &AttributedListener, filter_lc: &str) -> bool {
    if filter_lc.is_empty() {
        return true;
    }
    l.listener.command_name.to_lowercase().contains(filter_lc)
        || l.listener.port.to_string().contains(filter_lc)
        || l.listener.pid.to_string().contains(filter_lc)
        || l.listener
            .cwd
            .as_ref()
            .map(|p| p.to_string_lossy().to_lowercase().contains(filter_lc))
            .unwrap_or(false)
        || l.repo_name
            .as_ref()
            .map(|n| n.to_lowercase().contains(filter_lc))
            .unwrap_or(false)
        || l.worktree_branch
            .as_ref()
            .map(|n| n.to_lowercase().contains(filter_lc))
            .unwrap_or(false)
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
    ui.horizontal(|ui| {
        let running_log = snap
            .active_runs
            .values()
            .find(|r| r.worktree_path == w.path)
            .map(|r| r.log_path.clone());
        if let Some(log_path) = running_log {
            if theme::painted_logs_button(ui, theme::weak_text())
                .on_hover_text(format!("View logs — {}", log_path.display()))
                .clicked()
            {
                app.open_log_file(&log_path);
            }
        }
        if let Some(repo) = repo {
            if ui
                .small_button("Rename")
                .on_hover_text("Rename Switchbard label")
                .clicked()
            {
                pending.open_rename_worktree = Some((repo.clone(), w.clone()));
            }
        }
        if !is_primary {
            let m = snap.meta.get(&w.path).cloned().unwrap_or_default();
            let facts = removal_facts(
                is_primary,
                &m,
                super::attached_processes(snap, &w.path, listener_count),
            );
            let verdict =
                RemovalSafety::evaluate(&facts, RemovalIntent::WorktreeAndBranch).verdict();
            let base = match verdict {
                RemovalVerdict::Blocked => theme::amber(),
                _ => theme::weak_text(),
            };
            let resp = theme::painted_trash_button(ui, base);
            if resp.on_hover_text("Remove worktree…").clicked() {
                pending.open_remove_worktree =
                    Some((w.repo_name.clone(), w.path.clone(), w.branch.clone()));
            }
            ui.add_space(2.0);
            bulk_remove::render_select_checkbox(
                ui,
                w,
                is_primary,
                &mut app.bulk_selected_worktrees,
            );
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

#[cfg(test)]
mod tests {
    //! Tiered Open-button port resolution — unchanged decision logic from
    //! the retired swimlane view's own test module (see `open_port_for_
    //! running`'s doc for the four-tier summary this exercises): the four
    //! tiers must each be exercised, and the "exactly one unclaimed
    //! listener" guard on the worktree-claim tier must hold against
    //! multi-candidate ambiguity. Only the assertions changed shape —
    //! `OpenPortHint` dropped its `source` field (the tooltip no longer
    //! differentiates tiers, see the type's own doc), so these check `.port`
    //! and presence/absence only.

    use super::*;
    use crate::runtime::ActiveRun;
    use std::path::PathBuf;
    use std::time::Instant;
    use switchbard_core::types::LocalListener;
    use switchbard_core::DetectedService;

    fn wt_path() -> PathBuf {
        PathBuf::from("/repo/wt")
    }

    fn other_wt_path() -> PathBuf {
        PathBuf::from("/repo/other")
    }

    fn listener(pid: u32, pgid: i32, port: u16) -> AttributedListener {
        AttributedListener {
            repo_name: Some("repo".into()),
            worktree_path: Some(wt_path()),
            worktree_branch: Some("main".into()),
            listener: LocalListener {
                pid,
                pgid,
                port,
                command_name: "node".into(),
                cwd: Some(wt_path()),
            },
        }
    }

    fn active_run(service: &str, pgid: i32, worktree: PathBuf) -> ActiveRun {
        ActiveRun {
            worktree_path: worktree,
            service_name: service.into(),
            command: "cmd".into(),
            pid: 1,
            pgid,
            started_at: Instant::now(),
            log_path: PathBuf::new(),
        }
    }

    fn resolved_service(name: &str, expected_port: Option<u16>) -> ResolvedService {
        ResolvedService {
            canonical_name: name.into(),
            expected_port,
            likelihood: ServerLikelihood::Server,
            entry_points: vec![DetectedService {
                name: name.into(),
                command: name.into(),
                cwd_rel: PathBuf::from("."),
                source: switchbard_core::ServiceSource::NodeScript,
                source_file: PathBuf::from("package.json"),
                likelihood: ServerLikelihood::Server,
                expected_port,
            }],
        }
    }

    #[test]
    fn tier_a_pgid_match_wins() {
        let mut snap = Snapshot::default();
        snap.ports_by_pgid.insert(42, vec![6006]);
        let hint =
            open_port_for_running(42, &wt_path(), &resolved_service("storybook", None), &snap)
                .unwrap();
        assert_eq!(hint.port, 6006);
    }

    #[test]
    fn tier_b_unclaimed_worktree_listener_when_pgid_misses() {
        // Storybook scenario: Switchbard launched the run under pgid 42, but the
        // actual worker bound :6006 under pgid 99 after detaching.
        let mut snap = Snapshot::default();
        snap.listeners_by_wt
            .insert(wt_path(), vec![listener(123, 99, 6006)]);
        let hint =
            open_port_for_running(42, &wt_path(), &resolved_service("storybook", None), &snap)
                .unwrap();
        assert_eq!(hint.port, 6006);
    }

    #[test]
    fn tier_b_skips_listeners_claimed_by_another_active_run() {
        // A second service is already running in the same worktree and owns
        // the only listener. Don't misattribute.
        let mut snap = Snapshot::default();
        snap.listeners_by_wt
            .insert(wt_path(), vec![listener(123, 50, 5173)]);
        snap.active_runs
            .insert(50, active_run("other", 50, wt_path()));
        // No declared port and no known default → tier should return None.
        let hint = open_port_for_running(42, &wt_path(), &resolved_service("custom", None), &snap);
        assert!(hint.is_none());
    }

    #[test]
    fn tier_b_requires_exactly_one_unclaimed_candidate() {
        // Two unclaimed listeners — we can't tell which is ours.
        let mut snap = Snapshot::default();
        snap.listeners_by_wt.insert(
            wt_path(),
            vec![listener(123, 99, 6006), listener(124, 100, 5173)],
        );
        let hint = open_port_for_running(42, &wt_path(), &resolved_service("custom", None), &snap);
        assert!(hint.is_none());
    }

    #[test]
    fn tier_b_ignores_other_worktrees() {
        // A listener on a different worktree should not satisfy tier B for ours.
        let mut snap = Snapshot::default();
        snap.listeners_by_wt
            .insert(other_wt_path(), vec![listener(123, 99, 6006)]);
        let hint = open_port_for_running(42, &wt_path(), &resolved_service("custom", None), &snap);
        assert!(hint.is_none());
    }

    #[test]
    fn tier_c_declared_port_fallback() {
        let snap = Snapshot::default();
        let hint = open_port_for_running(
            42,
            &wt_path(),
            &resolved_service("custom", Some(7777)),
            &snap,
        )
        .unwrap();
        assert_eq!(hint.port, 7777);
    }

    #[test]
    fn tier_d_known_default_for_canonical_name() {
        let snap = Snapshot::default();
        let hint =
            open_port_for_running(42, &wt_path(), &resolved_service("storybook", None), &snap)
                .unwrap();
        assert_eq!(hint.port, 6006);
    }

    #[test]
    fn returns_none_when_no_tier_matches() {
        let snap = Snapshot::default();
        let hint = open_port_for_running(
            42,
            &wt_path(),
            &resolved_service("unknown-tool", None),
            &snap,
        );
        assert!(hint.is_none());
    }

    #[test]
    fn pgid_match_beats_declared_port() {
        // If we have a real pgid-matched listener, prefer that over the
        // command-line declaration — even when they disagree (e.g. user
        // passed --port 6006 but Storybook bumped to 6007 because 6006 was
        // taken).
        let mut snap = Snapshot::default();
        snap.ports_by_pgid.insert(42, vec![6007]);
        let hint = open_port_for_running(
            42,
            &wt_path(),
            &resolved_service("storybook", Some(6006)),
            &snap,
        )
        .unwrap();
        assert_eq!(hint.port, 6007);
    }
}
