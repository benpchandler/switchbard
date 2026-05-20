//! Servers view — per-repo, per-worktree table of detected services with
//! Start/Stop/Open actions and port-conflict pre-warning.

use crate::app::HiveApp;
use crate::runtime::{ActiveRun, RowState};
use crate::ui::column_widths::{self, CellFont};
use crate::ui::components::{
    mono_label, repo_section_header, repo_section_separator, status_pill, strings, table_shell,
    weak_dash, weak_dots, Chip, StatusKind,
};
use crate::ui::theme;
use eframe::egui;
use egui_extras::Column;
use hive_core::{
    AttributedListener, DetectedService, ServerLikelihood, ServiceSource, WorktreeRef,
};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

/// User actions queued during table rendering; applied after the closure
/// closes so we don't borrow `app` twice.
#[derive(Default)]
struct PendingActions {
    start: Option<(PathBuf, DetectedService)>,
    stop: Option<(i32, String)>,
    open: Option<u16>,
}

pub fn render(app: &mut HiveApp, ctx: &egui::Context) {
    let snap = Snapshot::collect(app);
    let widths = SvColumnWidths::compute(ctx, &snap, app.show_non_servers);
    let mut pending = PendingActions::default();

    let external_live = snap.count_external_live();
    egui::CentralPanel::default().show(ctx, |ui| {
        let mut summary = format!(
            "{} services detected across {}/{} worktrees · {} running",
            snap.detected_total,
            snap.known_paths,
            snap.worktrees.len(),
            snap.active_runs.len(),
        );
        if external_live > 0 {
            summary.push_str(&format!(" · {external_live} external"));
        }
        ui.label(egui::RichText::new(summary).weak());
        ui.add_space(8.0);

        let mut wts_by_repo: HashMap<&str, Vec<&WorktreeRef>> = HashMap::new();
        for w in &snap.worktrees {
            wts_by_repo.entry(w.repo_name.as_str()).or_default().push(w);
        }

        egui::ScrollArea::vertical()
            .id_salt("servers_outer_scroll")
            .show(ui, |ui| {
                let mut first = true;
                for repo in &snap.repos {
                    let Some(wts) = wts_by_repo.get(repo.name.as_str()) else {
                        continue;
                    };
                    first = repo_section_separator(ui, first);
                    ui.push_id(format!("server_repo_{}", repo.name), |ui| {
                        render_repo_section(
                            ui,
                            repo,
                            wts,
                            &snap,
                            app.show_non_servers,
                            widths,
                            &mut pending,
                        );
                    });
                }
            });
    });

    if let Some((path, svc)) = pending.start {
        app.spawn_start(path, svc, ctx);
    }
    if let Some((pgid, name)) = pending.stop {
        app.spawn_stop_run(pgid, name, ctx);
    }
    if let Some(port) = pending.open {
        app.open_in_browser(port);
    }
}

/// All inputs the table render needs, locked-snapshotted up front so the
/// render closures hold no Mutex.
struct Snapshot {
    services: HashMap<PathBuf, Vec<DetectedService>>,
    active_runs: HashMap<i32, ActiveRun>,
    by_port: HashMap<u16, AttributedListener>,
    ports_by_pgid: HashMap<i32, Vec<u16>>,
    worktrees: Vec<WorktreeRef>,
    repos: Vec<hive_core::Repo>,
    filter_lc: String,
    detected_total: usize,
    known_paths: usize,
}

impl Snapshot {
    fn collect(app: &HiveApp) -> Self {
        let services: HashMap<PathBuf, Vec<DetectedService>> = app.services.lock().unwrap().clone();
        let active_runs: HashMap<i32, ActiveRun> = app.active_runs.lock().unwrap().clone();
        let attributed_listeners: Vec<AttributedListener> =
            app.state.lock().unwrap().listeners.clone();
        let filter_lc = app.server_filter.to_lowercase();

        // Index listeners two ways: by pgid (Hive-managed runs — we know their
        // pgid) and by port (for blocker / external-live detection).
        let mut ports_by_pgid: HashMap<i32, Vec<u16>> = HashMap::new();
        let mut by_port: HashMap<u16, AttributedListener> = HashMap::new();
        for al in &attributed_listeners {
            ports_by_pgid
                .entry(al.listener.pgid)
                .or_default()
                .push(al.listener.port);
            by_port
                .entry(al.listener.port)
                .or_insert_with(|| al.clone());
        }
        for v in ports_by_pgid.values_mut() {
            v.sort();
            v.dedup();
        }

        let detected_total: usize = services.values().map(|v| v.len()).sum();
        let known_paths = services.len();
        Self {
            services,
            active_runs,
            by_port,
            ports_by_pgid,
            worktrees: app.worktrees_snapshot(),
            repos: app.repos_snapshot(),
            filter_lc,
            detected_total,
            known_paths,
        }
    }

    fn run_for(&self, wt_path: &Path, service_name: &str) -> Option<&ActiveRun> {
        self.active_runs
            .values()
            .find(|r| r.worktree_path == wt_path && r.service_name == service_name)
    }

    /// Count detected services whose expected port is currently bound by a
    /// process Hive didn't start — these are "live (external)" rows. Useful
    /// for the summary so the header doesn't claim "0 running" when the
    /// table clearly shows live services.
    fn count_external_live(&self) -> usize {
        let mut n = 0usize;
        for (wt_path, svcs) in &self.services {
            for svc in svcs {
                let Some(port) = svc.expected_port else {
                    continue;
                };
                let run = self.run_for(wt_path, &svc.name);
                let containerized = svc.source == ServiceSource::DockerCompose;
                if matches!(
                    RowState::compute(Some(port), wt_path, run, &self.by_port, containerized),
                    RowState::ExternalLive { .. }
                ) {
                    n += 1;
                }
            }
        }
        n
    }
}

fn render_repo_section(
    ui: &mut egui::Ui,
    repo: &hive_core::Repo,
    wts: &[&WorktreeRef],
    snap: &Snapshot,
    show_non_servers: bool,
    widths: SvColumnWidths,
    pending: &mut PendingActions,
) {
    let mut repo_running = 0usize;
    let mut repo_services_total = 0usize;
    for w in wts.iter() {
        let svcs = snap.services.get(&w.path).cloned().unwrap_or_default();
        repo_services_total += svcs.len();
        for s in &svcs {
            if snap.run_for(&w.path, &s.name).is_some() {
                repo_running += 1;
            }
        }
    }

    let subtitle = format!("({} svc · {} wt)", repo_services_total, wts.len());
    let running_chip_text;
    let chips: Vec<Chip<'_>> = if repo_running > 0 {
        running_chip_text = format!("{repo_running} running");
        vec![Chip {
            color: theme::GREEN,
            text: &running_chip_text,
        }]
    } else {
        Vec::new()
    };
    repo_section_header(ui, &repo.name, &subtitle, &chips, None);

    table_shell(ui, format!("server_table_{}", repo.name))
        // Short data columns get widths pre-measured across the whole tab so
        // every per-repo table lines up. COMMAND no longer has its own
        // column — it renders on a second line inside the SERVICE cell so
        // the full text is always visible without claiming horizontal room.
        .column(Column::initial(widths.branch).at_least(100.0))
        .column(Column::initial(widths.service).at_least(160.0))
        .column(Column::initial(widths.state).at_least(120.0))
        .column(Column::initial(widths.ports).at_least(70.0))
        .column(Column::remainder().at_least(160.0)) // actions
        .header(24.0, |mut h| {
            h.col(|ui| {
                ui.strong(strings::COL_BRANCH);
            });
            h.col(|ui| {
                ui.strong(strings::COL_SERVICE);
            });
            h.col(|ui| {
                ui.strong(strings::COL_STATE);
            });
            h.col(|ui| {
                ui.strong(strings::COL_PORTS);
            });
            h.col(|ui| {
                ui.strong(strings::COL_ACTIONS);
            });
        })
        .body(|mut body| {
            for w in wts {
                let branch = w.branch.clone().unwrap_or_else(|| "(detached)".into());
                let svcs = snap.services.get(&w.path).cloned().unwrap_or_default();
                if svcs.is_empty() {
                    render_empty_worktree_row(&mut body, w, &branch, &snap.filter_lc);
                    continue;
                }
                for svc in &svcs {
                    if should_skip_service(svc, w, snap, show_non_servers) {
                        continue;
                    }
                    let row_text =
                        format!("{} {} {} {}", w.repo_name, branch, svc.name, svc.command);
                    if !snap.filter_lc.is_empty()
                        && !row_text.to_lowercase().contains(&snap.filter_lc)
                    {
                        continue;
                    }
                    let run = snap.run_for(&w.path, &svc.name);
                    let containerized = svc.source == ServiceSource::DockerCompose;
                    let row_state = RowState::compute(
                        svc.expected_port,
                        &w.path,
                        run,
                        &snap.by_port,
                        containerized,
                    );

                    body.row(SERVICE_ROW_HEIGHT, |mut r| {
                        r.col(|ui| {
                            ui.add(egui::Label::new(&branch).truncate())
                                .on_hover_text(&branch);
                        });
                        r.col(|ui| render_service_cell(ui, svc, &row_state));
                        r.col(|ui| render_state_cell(ui, &row_state));
                        r.col(|ui| render_ports_cell(ui, &row_state, &snap.ports_by_pgid));
                        r.col(|ui| {
                            render_actions_cell(
                                ui,
                                svc,
                                w,
                                &row_state,
                                &snap.ports_by_pgid,
                                pending,
                            );
                        });
                    });
                }
            }
        });
}

/// Two-line rows (service name on top, command monospace below) need more
/// vertical room than the prior 24px single-line cells.
const SERVICE_ROW_HEIGHT: f32 = 40.0;

fn render_empty_worktree_row(
    body: &mut egui_extras::TableBody<'_>,
    w: &WorktreeRef,
    branch: &str,
    filter_lc: &str,
) {
    // Surface "no services detected" only when the user is actually searching —
    // otherwise it's just noise.
    if filter_lc.is_empty() {
        return;
    }
    let row_text = format!("{} {} {}", w.repo_name, branch, w.path.display());
    if !row_text.to_lowercase().contains(filter_lc) {
        return;
    }
    // Five columns: WORKTREE, SERVICE, STATE, PORTS, ACTIONS
    body.row(24.0, |mut r| {
        r.col(|ui| {
            ui.label(branch);
        });
        r.col(|ui| {
            ui.label(egui::RichText::new("(none detected)").weak());
        });
        r.col(|_| {});
        r.col(|_| {});
        r.col(|_| {});
    });
}

fn should_skip_service(
    svc: &DetectedService,
    w: &WorktreeRef,
    snap: &Snapshot,
    show_non_servers: bool,
) -> bool {
    // Always show NotServer rows for a running run (so the user can Stop it).
    // For idle rows, hide unless the toggle is on.
    if show_non_servers {
        return false;
    }
    if svc.likelihood != ServerLikelihood::NotServer {
        return false;
    }
    snap.run_for(&w.path, &svc.name).is_none()
}

fn render_service_cell(ui: &mut egui::Ui, svc: &DetectedService, row_state: &RowState) {
    ui.vertical(|ui| {
        // Line 1: state-driven dot + service name (+ small classification
        // marker if the command isn't a clear server).
        ui.horizontal(|ui| {
            theme::painted_dot(ui, state_dot_color(row_state))
                .on_hover_text(state_dot_legend(row_state));
            let name_text = match svc.likelihood {
                ServerLikelihood::NotServer => egui::RichText::new(&svc.name).weak().italics(),
                _ => egui::RichText::new(&svc.name),
            };
            ui.add(egui::Label::new(name_text).truncate())
                .on_hover_text(&svc.name);
            match svc.likelihood {
                ServerLikelihood::Server => {}
                ServerLikelihood::Maybe => {
                    ui.colored_label(theme::AMBER_QUESTION, "?")
                        .on_hover_text("ambiguous — could be a server or a one-shot");
                }
                ServerLikelihood::NotServer => {
                    ui.label(egui::RichText::new("(non-server)").small().weak())
                        .on_hover_text(
                            "classified as a one-shot (test / build / lint), not a long-lived \
                             server — shown because 'Show non-servers' is enabled or it's \
                             currently running",
                        );
                }
            }
        });
        // Line 2: full command, monospace, weak — truncates with ellipsis at
        // the cell's right edge; full text is in the hover.
        ui.add(egui::Label::new(egui::RichText::new(&svc.command).monospace().weak()).truncate())
            .on_hover_text(&svc.command);
    });
}

/// Map a `RowState` to the dot color used in the SERVICE cell — encodes
/// actual run state so the visual matches the STATE column wording.
fn state_dot_color(row_state: &RowState) -> egui::Color32 {
    match row_state {
        RowState::Running { .. } => theme::GREEN,
        RowState::ExternalLive { .. } => theme::SKY,
        RowState::Blocked { .. } => theme::WARN_ORANGE,
        RowState::Idle => egui::Color32::GRAY,
    }
}

/// Hover text for the state dot — small key so the color encoding is
/// discoverable without a separate legend.
fn state_dot_legend(row_state: &RowState) -> &'static str {
    match row_state {
        RowState::Running { .. } => "running — started by Hive",
        RowState::ExternalLive { .. } => {
            "live — running, but not started by Hive (existing terminal session, \
             container runtime, system service, etc.)"
        }
        RowState::Blocked { .. } => "blocked — another process holds the port",
        RowState::Idle => "idle — not running",
    }
}

fn render_state_cell(ui: &mut egui::Ui, row_state: &RowState) {
    match row_state {
        RowState::Running {
            pid, started_at, ..
        } => {
            let text = format!("running · pid {pid} · {}", uptime_short(*started_at));
            status_pill(ui, StatusKind::Good, text, Some("started by Hive"));
        }
        RowState::ExternalLive { port, pid } => {
            let text = format!("live (external) · :{port} · pid {pid}");
            status_pill(
                ui,
                StatusKind::Info,
                text,
                Some(
                    "a process bound to this command's expected port is already running from \
                     this worktree (not started by Hive)",
                ),
            );
        }
        RowState::Blocked {
            port,
            pid,
            holder_label,
        } => {
            let text = format!("blocked · :{port} held by pid {pid} ({holder_label})");
            status_pill(
                ui,
                StatusKind::Danger,
                text,
                Some(
                    "another listener is already bound to this command's expected port — \
                     Start would fail with EADDRINUSE",
                ),
            );
        }
        RowState::Idle => {
            ui.label(egui::RichText::new("idle").weak());
        }
    }
}

fn render_ports_cell(
    ui: &mut egui::Ui,
    row_state: &RowState,
    ports_by_pgid: &HashMap<i32, Vec<u16>>,
) {
    match row_state {
        RowState::Running { pgid, .. } => {
            let ports = ports_by_pgid.get(pgid).cloned().unwrap_or_default();
            if ports.is_empty() {
                weak_dots(ui);
            } else {
                let txt = ports
                    .iter()
                    .map(|p| p.to_string())
                    .collect::<Vec<_>>()
                    .join(", ");
                mono_label(ui, &txt, None);
            }
        }
        RowState::ExternalLive { port, .. } => {
            mono_label(ui, &port.to_string(), None);
        }
        RowState::Blocked { port, .. } => {
            mono_label(ui, &format!("(want :{port})"), Some(theme::WARN_ORANGE));
        }
        RowState::Idle => weak_dash(ui),
    }
}

fn render_actions_cell(
    ui: &mut egui::Ui,
    svc: &DetectedService,
    w: &WorktreeRef,
    row_state: &RowState,
    ports_by_pgid: &HashMap<i32, Vec<u16>>,
    pending: &mut PendingActions,
) {
    match row_state {
        RowState::Running { pgid, .. } => {
            if ui
                .add(egui::Button::new("Stop").fill(theme::DANGER))
                .clicked()
            {
                pending.stop = Some((*pgid, svc.name.clone()));
            }
            let ports = ports_by_pgid.get(pgid).cloned().unwrap_or_default();
            let open_label = match ports.first() {
                Some(p) => format!("Open :{p}"),
                None => "Open".into(),
            };
            if ui
                .add_enabled(!ports.is_empty(), egui::Button::new(open_label))
                .clicked()
            {
                if let Some(p) = ports.first() {
                    pending.open = Some(*p);
                }
            }
        }
        RowState::ExternalLive { port, .. } => {
            if ui.button(format!("Open :{port}")).clicked() {
                pending.open = Some(*port);
            }
        }
        RowState::Blocked { .. } => {
            ui.add_enabled(false, egui::Button::new("Start"))
                .on_disabled_hover_text(
                    "port already held; stop or kill the blocking process first",
                );
        }
        RowState::Idle => {
            if ui.button("Start").clicked() {
                pending.start = Some((w.path.clone(), svc.clone()));
            }
        }
    }
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

/// Shared widths for the short columns in the Servers table, pre-measured
/// once over every visible row in the tab so the per-repo tables line up.
/// ACTIONS is excluded — it claims the Remainder column. COMMAND is
/// excluded too: it now renders inline under the service name, no column.
#[derive(Debug, Clone, Copy)]
struct SvColumnWidths {
    branch: f32,
    service: f32,
    state: f32,
    ports: f32,
}

impl SvColumnWidths {
    fn compute(ctx: &egui::Context, snap: &Snapshot, show_non_servers: bool) -> Self {
        use crate::ui::components::strings as s;

        let mut branches: Vec<String> = Vec::new();
        let mut services: Vec<String> = Vec::new();
        let mut states: Vec<String> = Vec::new();
        let mut ports_strs: Vec<String> = Vec::new();

        for w in &snap.worktrees {
            let branch = w.branch.clone().unwrap_or_else(|| "(detached)".into());
            let svcs = snap.services.get(&w.path).cloned().unwrap_or_default();
            if svcs.is_empty() {
                branches.push(branch.clone());
                continue;
            }
            for svc in &svcs {
                if should_skip_service(svc, w, snap, show_non_servers) {
                    continue;
                }
                let run = snap.run_for(&w.path, &svc.name);
                let containerized = svc.source == ServiceSource::DockerCompose;
                let row_state = RowState::compute(
                    svc.expected_port,
                    &w.path,
                    run,
                    &snap.by_port,
                    containerized,
                );
                branches.push(branch.clone());
                services.push(svc.name.clone());
                states.push(state_display_text(&row_state));
                ports_strs.push(ports_display_text(&row_state, &snap.ports_by_pgid));
            }
        }

        let branch = column_widths::column_width_clamped(
            ctx,
            std::iter::once(s::COL_BRANCH).chain(branches.iter().map(String::as_str)),
            CellFont::Proportional,
            100.0,
            240.0,
        );
        let service = column_widths::column_width_clamped(
            ctx,
            std::iter::once(s::COL_SERVICE).chain(services.iter().map(String::as_str)),
            CellFont::Proportional,
            // service cell starts with a painted dot + small gap → reserve ~16px
            // for the icon column even when no service text is wider than the
            // header.
            160.0,
            260.0,
        );
        let state = column_widths::column_width(
            ctx,
            std::iter::once(s::COL_STATE).chain(states.iter().map(String::as_str)),
            CellFont::Proportional,
            120.0,
        );
        let ports = column_widths::column_width(
            ctx,
            std::iter::once(s::COL_PORTS).chain(ports_strs.iter().map(String::as_str)),
            CellFont::Monospace,
            70.0,
        );

        Self {
            branch,
            service,
            state,
            ports,
        }
    }
}

/// Text representation of `RowState` used for width measurement only — must
/// match what `render_state_cell` actually renders.
fn state_display_text(row_state: &RowState) -> String {
    match row_state {
        RowState::Running {
            pid, started_at, ..
        } => format!("running · pid {pid} · {}", uptime_short(*started_at)),
        RowState::ExternalLive { port, pid } => format!("live (external) · :{port} · pid {pid}"),
        RowState::Blocked {
            port,
            pid,
            holder_label,
        } => format!("blocked · :{port} held by pid {pid} ({holder_label})"),
        RowState::Idle => "idle".to_string(),
    }
}

/// Text representation of the ports cell — must match `render_ports_cell`.
fn ports_display_text(row_state: &RowState, ports_by_pgid: &HashMap<i32, Vec<u16>>) -> String {
    match row_state {
        RowState::Running { pgid, .. } => {
            let ports = ports_by_pgid.get(pgid).cloned().unwrap_or_default();
            ports
                .iter()
                .map(|p| p.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        }
        RowState::ExternalLive { port, .. } => port.to_string(),
        RowState::Blocked { port, .. } => format!("(want :{port})"),
        RowState::Idle => "—".to_string(),
    }
}
