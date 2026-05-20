//! Servers view — per-repo, per-worktree table of detected services with
//! Start/Stop/Open actions and port-conflict pre-warning.

use crate::app::HiveApp;
use crate::runtime::{ActiveRun, RowState};
use crate::ui::components::{
    mono_label, repo_section_header, repo_section_separator, status_pill, strings, table_shell,
    weak_dash, weak_dots, Chip, StatusKind,
};
use crate::ui::theme;
use eframe::egui;
use egui_extras::Column;
use hive_core::{AttributedListener, DetectedService, ServerLikelihood, WorktreeRef};
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
    let mut pending = PendingActions::default();

    egui::CentralPanel::default().show(ctx, |ui| {
        ui.label(
            egui::RichText::new(format!(
                "{} services detected across {}/{} worktrees · {} running",
                snap.detected_total,
                snap.known_paths,
                snap.worktrees.len(),
                snap.active_runs.len(),
            ))
            .weak(),
        );
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
}

fn render_repo_section(
    ui: &mut egui::Ui,
    repo: &hive_core::Repo,
    wts: &[&WorktreeRef],
    snap: &Snapshot,
    show_non_servers: bool,
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
        // Short data columns auto-fit; COMMAND is the only multi-line cell so
        // it claims the Remainder. STATE / PORTS / ACTIONS auto-fit.
        .column(Column::auto().at_least(120.0)) // worktree (branch)
        .column(Column::auto().at_least(120.0)) // service
        .column(Column::auto().at_least(110.0)) // state
        .column(Column::auto().at_least(70.0)) // ports
        .column(Column::auto().at_least(160.0)) // actions
        .column(Column::remainder().at_least(200.0)) // command (wraps)
        .header(24.0, |mut h| {
            h.col(|ui| {
                ui.strong(strings::COL_WORKTREE);
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
            h.col(|ui| {
                ui.strong(strings::COL_COMMAND);
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
                    let row_state =
                        RowState::compute(svc.expected_port, &w.path, run, &snap.by_port);
                    let row_height = estimate_row_height(&svc.command);

                    body.row(row_height, |mut r| {
                        r.col(|ui| {
                            ui.label(&branch);
                        });
                        r.col(|ui| render_service_cell(ui, svc));
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
                        r.col(|ui| render_command_cell(ui, &svc.command));
                    });
                }
            }
        });
}

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
    // Six columns: WORKTREE, SERVICE, STATE, PORTS, ACTIONS, COMMAND
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

fn render_service_cell(ui: &mut egui::Ui, svc: &DetectedService) {
    ui.horizontal(|ui| {
        match svc.likelihood {
            ServerLikelihood::Server => {
                theme::painted_dot(ui, theme::GREEN);
            }
            ServerLikelihood::Maybe => {
                ui.colored_label(theme::AMBER_QUESTION, "?")
                    .on_hover_text("ambiguous — could be a server or one-shot");
            }
            ServerLikelihood::NotServer => {
                theme::painted_x(ui, egui::Color32::DARK_GRAY);
            }
        }
        ui.label(&svc.name);
    });
}

fn render_command_cell(ui: &mut egui::Ui, command: &str) {
    ui.add(egui::Label::new(egui::RichText::new(command).monospace()).wrap());
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

/// Estimate the row height needed to display the Servers-table command cell
/// without clipping the wrapped text. egui_extras' TableBuilder needs a fixed
/// per-row height, but the wrapped label inside wants to grow — so we pre-
/// compute based on a conservative chars/line estimate. After the user resizes
/// the column the estimate may be too tall (harmless) or too short (clipped
/// — a known limitation; live width isn't exposed to the body callback).
fn estimate_row_height(command: &str) -> f32 {
    // COMMAND is now body-size monospace (was `.small()`), so each line is
    // taller and a wider char-pixel ratio. The remainder column for COMMAND
    // is at least 200px wide; mono body text is ~8px/char, so ~25 chars/line
    // is the conservative break point. Cap at 4 lines so a runaway command
    // doesn't blow up the table row.
    const CHARS_PER_LINE: usize = 25;
    const LINE_HEIGHT: f32 = 18.0;
    const MIN_ROW_HEIGHT: f32 = 24.0;
    const MAX_LINES: usize = 4;
    let lines = command
        .chars()
        .count()
        .div_ceil(CHARS_PER_LINE)
        .clamp(1, MAX_LINES);
    (lines as f32 * LINE_HEIGHT + 6.0).max(MIN_ROW_HEIGHT)
}
