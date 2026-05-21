//! Workspace view — single central panel with per-repo swimlane cards.
//!
//! Each repo is a Frame; inside it, every worktree is a row. A worktree
//! row is one of two shapes:
//!
//! - **Compact line** — the "boring" case (clean tree, no listeners, no
//!   running services, no recent activity). Branch + a couple of weak
//!   status words on one line.
//! - **Expanded body** — the "noteworthy" case. The compact line stays
//!   visible as the heading; below it sit two inline strips: services
//!   on top, listeners below. No tabs, no nested trees.
//!
//! `is_noteworthy` drives the default expansion, but `CollapsingState`
//! persists user overrides — click the chevron and the choice sticks
//! across frames.
//!
//! There's one filter input in the top bar. Filtering forces ancestors
//! open. An "Unattributed listeners" card sits at the bottom for OS-level
//! listeners that didn't attribute to any tracked worktree.

use crate::app::HiveApp;
use crate::runtime::{ActiveRun, ActivityLevel, RowState, WorktreeMeta};
use crate::ui::components::{
    branch_label, mono_label, path_cell, status_pill, weak_dots, Chip, StatusKind,
};
use crate::ui::theme;
use eframe::egui::{self, collapsing_header::CollapsingState};
use hive_core::{
    humanize_age, resolve, AttributedListener, DetectedService, Repo, ResolvedService,
    ServerLikelihood, ServiceSource, WorktreeRef,
};
use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::time::Instant;

pub mod tooltips;
use tooltips::{activity_tooltip, dirty_tooltip, drift_tooltip};

/// Actions queued during the walk; applied after the central panel
/// closure exits so we don't double-borrow `app`.
#[derive(Default)]
struct Pending {
    start: Option<(PathBuf, DetectedService)>,
    stop: Option<(i32, String)>,
    open: Option<u16>,
    kill: Option<i32>,
}

pub fn render(app: &mut HiveApp, ctx: &egui::Context) {
    let snap = Snapshot::collect(app);
    let mut pending = Pending::default();

    egui::CentralPanel::default().show(ctx, |ui| {
        render_summary(ui, &snap);
        ui.add_space(6.0);
        egui::ScrollArea::vertical()
            .id_salt("workspace_scroll")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for repo in &snap.repos {
                    let wts: Vec<&WorktreeRef> = snap
                        .worktrees
                        .iter()
                        .filter(|w| w.repo_name == repo.name)
                        .collect();
                    if wts.is_empty() {
                        continue;
                    }
                    if !snap.filter_lc.is_empty()
                        && !wts
                            .iter()
                            .any(|w| worktree_matches(w, &snap, &snap.filter_lc))
                    {
                        continue;
                    }
                    render_repo_card(ui, repo, &wts, &snap, app.show_non_servers, &mut pending);
                    ui.add_space(8.0);
                }
                if !snap.show_only_managed && !snap.unattributed.is_empty() {
                    render_unattributed_card(ui, &snap.unattributed, &mut pending);
                }
            });
    });

    apply_pending(app, ctx, pending);
    render_kill_all_modal(app, ctx);
}

fn apply_pending(app: &mut HiveApp, ctx: &egui::Context, p: Pending) {
    if let Some((path, svc)) = p.start {
        app.spawn_start(path, svc, ctx);
    }
    if let Some((pgid, name)) = p.stop {
        app.spawn_stop_run(pgid, name, ctx);
    }
    if let Some(port) = p.open {
        app.open_in_browser(port);
    }
    if let Some(pgid) = p.kill {
        app.spawn_kill(pgid, ctx);
    }
}

// ── snapshot ──────────────────────────────────────────────────────────────

struct Snapshot {
    repos: Vec<Repo>,
    worktrees: Vec<WorktreeRef>,
    meta: HashMap<PathBuf, WorktreeMeta>,
    services: HashMap<PathBuf, Vec<ResolvedService>>,
    listeners_by_wt: HashMap<PathBuf, Vec<AttributedListener>>,
    unattributed: Vec<AttributedListener>,
    active_runs: HashMap<i32, ActiveRun>,
    by_port: HashMap<u16, AttributedListener>,
    ports_by_pgid: HashMap<i32, Vec<u16>>,
    filter_lc: String,
    show_only_managed: bool,
    raw_detected_total: usize,
}

impl Snapshot {
    fn collect(app: &HiveApp) -> Self {
        let raw: HashMap<PathBuf, Vec<DetectedService>> = app.services.lock().unwrap().clone();
        let raw_detected_total: usize = raw.values().map(|v| v.len()).sum();
        let services: HashMap<PathBuf, Vec<ResolvedService>> =
            raw.into_iter().map(|(p, d)| (p, resolve(d))).collect();
        let meta = app.meta.lock().unwrap().clone();
        let active_runs = app.active_runs.lock().unwrap().clone();

        let attributed: Vec<AttributedListener> = app.state.lock().unwrap().listeners.clone();
        let mut listeners_by_wt: HashMap<PathBuf, Vec<AttributedListener>> = HashMap::new();
        let mut unattributed: Vec<AttributedListener> = Vec::new();
        let mut by_port: HashMap<u16, AttributedListener> = HashMap::new();
        let mut ports_by_pgid: HashMap<i32, Vec<u16>> = HashMap::new();
        for al in attributed {
            by_port
                .entry(al.listener.port)
                .or_insert_with(|| al.clone());
            ports_by_pgid
                .entry(al.listener.pgid)
                .or_default()
                .push(al.listener.port);
            match &al.worktree_path {
                Some(p) => listeners_by_wt.entry(p.clone()).or_default().push(al),
                None => unattributed.push(al),
            }
        }
        for v in ports_by_pgid.values_mut() {
            v.sort();
            v.dedup();
        }

        Self {
            repos: app.repos_snapshot(),
            worktrees: app.worktrees_snapshot(),
            meta,
            services,
            listeners_by_wt,
            unattributed,
            active_runs,
            by_port,
            ports_by_pgid,
            filter_lc: app.filter.to_lowercase(),
            show_only_managed: app.show_only_managed,
            raw_detected_total,
        }
    }

    fn run_for_resolved(&self, wt_path: &Path, resolved: &ResolvedService) -> Option<&ActiveRun> {
        for ep in &resolved.entry_points {
            if let Some(run) = self.run_for(wt_path, &ep.name) {
                return Some(run);
            }
        }
        None
    }

    fn run_for(&self, wt_path: &Path, service_name: &str) -> Option<&ActiveRun> {
        self.active_runs
            .values()
            .find(|r| r.worktree_path == wt_path && r.service_name == service_name)
    }

    fn unique_pgids_in_filter(&self) -> Vec<i32> {
        let mut set: BTreeSet<i32> = BTreeSet::new();
        for v in self.listeners_by_wt.values() {
            for l in v {
                if listener_matches(l, &self.filter_lc) {
                    set.insert(l.listener.pgid);
                }
            }
        }
        if !self.show_only_managed {
            for l in &self.unattributed {
                if listener_matches(l, &self.filter_lc) {
                    set.insert(l.listener.pgid);
                }
            }
        }
        set.into_iter().collect()
    }
}

fn is_containerized(resolved: &ResolvedService) -> bool {
    resolved
        .entry_points
        .iter()
        .any(|ep| ep.source == ServiceSource::DockerCompose)
}

// ── header summary ───────────────────────────────────────────────────────

fn render_summary(ui: &mut egui::Ui, snap: &Snapshot) {
    let services_total: usize = snap.services.values().map(|v| v.len()).sum();
    let listeners_total: usize = snap
        .listeners_by_wt
        .values()
        .map(|v| v.len())
        .sum::<usize>()
        + snap.unattributed.len();
    let running = snap.active_runs.len();
    let mut external = 0usize;
    for (wt_path, list) in &snap.services {
        for resolved in list {
            let Some(port) = resolved.expected_port else {
                continue;
            };
            let run = snap.run_for_resolved(wt_path, resolved);
            let c = is_containerized(resolved);
            if matches!(
                RowState::compute(Some(port), wt_path, run, &snap.by_port, c),
                RowState::ExternalLive { .. }
            ) {
                external += 1;
            }
        }
    }
    let mut s = format!(
        "{} repos · {} worktrees · {} services ({} raw entries) · {} listeners",
        snap.repos.len(),
        snap.worktrees.len(),
        services_total,
        snap.raw_detected_total,
        listeners_total,
    );
    if running > 0 {
        s.push_str(&format!(" · {running} running"));
    }
    if external > 0 {
        s.push_str(&format!(" · {external} external"));
    }
    ui.label(egui::RichText::new(s).weak());
}

// ── repo card ────────────────────────────────────────────────────────────

fn render_repo_card(
    ui: &mut egui::Ui,
    repo: &Repo,
    wts: &[&WorktreeRef],
    snap: &Snapshot,
    show_non_servers: bool,
    pending: &mut Pending,
) {
    let mut listening = 0usize;
    let mut dirty = 0usize;
    let mut drifted = 0usize;
    for w in wts {
        listening += snap
            .listeners_by_wt
            .get(&w.path)
            .map(|v| v.len())
            .unwrap_or(0);
        if let Some(m) = snap.meta.get(&w.path) {
            if m.is_dirty() == Some(true) {
                dirty += 1;
            }
            if m.ahead.unwrap_or(0) + m.behind.unwrap_or(0) > 0 {
                drifted += 1;
            }
        }
    }

    egui::Frame::group(ui.style())
        .inner_margin(egui::Margin::same(10.0))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                if listening > 0 {
                    theme::painted_dot_pulse(ui, theme::GREEN, listening);
                } else {
                    theme::painted_dot(ui, egui::Color32::GRAY);
                }
                ui.add_space(2.0);
                ui.heading(&repo.name);
                ui.label(egui::RichText::new(format!("{} wt", wts.len())).weak());
                // Chips quiet down: dirty/drifted only when the repo has more
                // worktrees than the eye can summarize at a glance. Listener
                // count is on the dot's pulse, no chip needed.
                if wts.len() > 3 {
                    let chip_storage = build_chips(dirty, drifted);
                    let chips: Vec<Chip<'_>> = chip_storage
                        .iter()
                        .map(|(c, t)| Chip {
                            color: *c,
                            text: t.as_str(),
                        })
                        .collect();
                    if !chips.is_empty() {
                        ui.separator();
                    }
                    for c in &chips {
                        ui.colored_label(c.color, c.text);
                    }
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        egui::RichText::new(repo.path.display().to_string())
                            .weak()
                            .small(),
                    );
                });
            });
            ui.add_space(4.0);

            for w in wts {
                if !worktree_matches(w, snap, &snap.filter_lc) {
                    continue;
                }
                ui.push_id(format!("wt_{}", w.path.display()), |ui| {
                    render_worktree_row(ui, w, snap, show_non_servers, pending);
                });
            }
        });
}

fn build_chips(dirty: usize, drifted: usize) -> Vec<(egui::Color32, String)> {
    let mut chips = Vec::new();
    if dirty > 0 {
        chips.push((theme::AMBER, format!("{dirty} dirty")));
    }
    if drifted > 0 {
        chips.push((theme::LAVENDER, format!("{drifted} drifted")));
    }
    chips
}

// ── worktree row ─────────────────────────────────────────────────────────

fn render_worktree_row(
    ui: &mut egui::Ui,
    w: &WorktreeRef,
    snap: &Snapshot,
    show_non_servers: bool,
    pending: &mut Pending,
) {
    let m = snap.meta.get(&w.path).cloned().unwrap_or_default();
    let listeners = snap
        .listeners_by_wt
        .get(&w.path)
        .cloned()
        .unwrap_or_default();
    let svcs = snap.services.get(&w.path).cloned().unwrap_or_default();
    let any_running_or_external = svcs.iter().any(|resolved| {
        let run = snap.run_for_resolved(&w.path, resolved);
        let c = is_containerized(resolved);
        matches!(
            RowState::compute(resolved.expected_port, &w.path, run, &snap.by_port, c),
            RowState::Running { .. } | RowState::ExternalLive { .. }
        )
    });
    let noteworthy = is_noteworthy(&listeners, &m, any_running_or_external);
    let default_open = noteworthy || !snap.filter_lc.is_empty();

    let id = ui.make_persistent_id(format!("wt_row_{}", w.path.display()));
    let state = CollapsingState::load_with_default_open(ui.ctx(), id, default_open);
    state
        .show_header(ui, |ui| {
            render_worktree_summary_line(ui, w, &m, listeners.len(), &svcs, snap);
        })
        .body(|ui| {
            ui.add_space(2.0);
            let service_ports: std::collections::HashSet<u16> =
                svcs.iter().filter_map(|s| s.expected_port).collect();
            if !svcs.is_empty() {
                render_services_strip(ui, w, &svcs, snap, show_non_servers, pending);
            }
            if !listeners.is_empty() {
                render_listeners_strip(ui, &listeners, &service_ports, snap, pending);
            }
            if svcs.is_empty() && listeners.is_empty() {
                ui.label(egui::RichText::new("nothing detected here").weak());
            }
            ui.add_space(4.0);
        });
}

/// "Noteworthy" worktree (auto-expand). The rule: anything the user
/// might want to act on or react to.
fn is_noteworthy(
    listeners: &[AttributedListener],
    m: &WorktreeMeta,
    any_running_or_external: bool,
) -> bool {
    if !listeners.is_empty() || any_running_or_external {
        return true;
    }
    if m.is_dirty() == Some(true) {
        return true;
    }
    if m.ahead.unwrap_or(0) + m.behind.unwrap_or(0) > 0 {
        return true;
    }
    if let Some(act) = m.activity() {
        return matches!(act.level, ActivityLevel::Burst | ActivityLevel::Active);
    }
    false
}

fn render_worktree_summary_line(
    ui: &mut egui::Ui,
    w: &WorktreeRef,
    m: &WorktreeMeta,
    listener_count: usize,
    svcs: &[ResolvedService],
    snap: &Snapshot,
) {
    let (dot_color, pulse_count) = headline_dot(svcs, w, snap, listener_count);
    if pulse_count > 0 {
        theme::painted_dot_pulse(ui, dot_color, pulse_count);
    } else {
        theme::painted_dot(ui, dot_color);
    }
    ui.add_space(2.0);
    branch_label(ui, w.branch.as_deref());
    // Health zone: dirty appears only when dirty; drift only when non-zero;
    // listener count is on the dot tooltip already (no inline tag).
    render_health_inline(ui, m);
    render_activity_inline(ui, m);
    let _ = listener_count;
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        ui.label(
            egui::RichText::new(w.head.chars().take(8).collect::<String>())
                .monospace()
                .weak()
                .small(),
        )
        .on_hover_text(&w.head);
    });
}

fn headline_dot(
    svcs: &[ResolvedService],
    w: &WorktreeRef,
    snap: &Snapshot,
    listener_count: usize,
) -> (egui::Color32, usize) {
    let mut running = 0usize;
    let mut external = 0usize;
    for resolved in svcs {
        let run = snap.run_for_resolved(&w.path, resolved);
        let c = is_containerized(resolved);
        match RowState::compute(resolved.expected_port, &w.path, run, &snap.by_port, c) {
            RowState::Running { .. } => running += 1,
            RowState::ExternalLive { .. } => external += 1,
            _ => {}
        }
    }
    if running > 0 || listener_count > 0 {
        return (theme::GREEN, listener_count.max(running));
    }
    if external > 0 {
        return (theme::SKY, 0);
    }
    if let Some(m) = snap.meta.get(&w.path) {
        if m.is_dirty() == Some(true) {
            return (theme::AMBER, 0);
        }
        if m.ahead.unwrap_or(0) + m.behind.unwrap_or(0) > 0 {
            return (theme::LAVENDER, 0);
        }
    }
    (egui::Color32::GRAY, 0)
}

/// One inline "health" zone: dirty + drift on a single line. Both fields
/// are silently absent when in their default state (clean / in sync) — the
/// absence of a token IS the "everything fine" signal. An em-dash shows
/// when both are absent so the eye still has an anchor.
fn render_health_inline(ui: &mut egui::Ui, m: &WorktreeMeta) {
    let dirty = matches!(m.is_dirty(), Some(true));
    let drift = match (m.ahead, m.behind) {
        (Some(a), Some(b)) if a + b > 0 => Some((a, b)),
        _ => None,
    };
    if !dirty && drift.is_none() {
        ui.label(egui::RichText::new("—").weak())
            .on_hover_text("clean · in sync");
        return;
    }
    if dirty {
        let tip = dirty_tooltip(m.dirty_files.as_deref().unwrap_or(&[]));
        status_pill(ui, StatusKind::Warn, "dirty", Some(&tip));
    }
    if let Some((a, b)) = drift {
        mono_label(ui, &format!("+{a}/-{b}"), Some(theme::LAVENDER)).on_hover_text(drift_tooltip(
            a,
            b,
            m.drift_detail.as_ref(),
            m.fetch_unix,
        ));
    }
}

fn render_activity_inline(ui: &mut egui::Ui, m: &WorktreeMeta) {
    let Some(act) = m.activity() else {
        weak_dots(ui);
        return;
    };
    let (kind, label) = match act.level {
        ActivityLevel::Burst => (StatusKind::Good, "Burst"),
        ActivityLevel::Active => (StatusKind::Good, "Active"),
        ActivityLevel::Slow => (StatusKind::Warn, "Slow"),
        ActivityLevel::Idle => (StatusKind::Neutral, "Idle"),
    };
    let txt = if act.count_1h > 0 {
        format!("{label} +{}/1h", act.count_1h)
    } else if act.count_24h > 0 {
        format!("{label} +{}/24h", act.count_24h)
    } else {
        label.to_string()
    };
    let age_suffix = m.head_commit_unix.map(humanize_age).unwrap_or_default();
    let full = if age_suffix.is_empty() {
        txt
    } else {
        format!("{txt} · {age_suffix}")
    };
    let tip = activity_tooltip(&act, m.recent_commits.as_deref().unwrap_or(&[]));
    status_pill(ui, kind, full, Some(&tip));
}

// ── services strip ──────────────────────────────────────────────────────

fn render_services_strip(
    ui: &mut egui::Ui,
    w: &WorktreeRef,
    svcs: &[ResolvedService],
    snap: &Snapshot,
    show_non_servers: bool,
    pending: &mut Pending,
) {
    let visible: Vec<&ResolvedService> = svcs
        .iter()
        .filter(|s| !should_skip_service(s, w, snap, show_non_servers))
        .filter(|s| service_matches_filter(s, w, &snap.filter_lc))
        .collect();
    if visible.is_empty() {
        return;
    }
    // No sub-label — indent + dot-color + the Start/Stop/Open verbs
    // identify these as service rows. Keeping the strip silent.
    ui.indent("svc_indent", |ui| {
        for resolved in visible {
            render_service_line(ui, w, resolved, snap, pending);
        }
    });
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

fn service_matches_filter(resolved: &ResolvedService, w: &WorktreeRef, filter_lc: &str) -> bool {
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

fn render_service_line(
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

    ui.horizontal(|ui| {
        // Likelihood "?" marker dropped — the dot color (and its hover)
        // already encodes Ambiguous vs Server vs NotServer.
        theme::painted_dot(ui, state_dot_color(&row_state))
            .on_hover_text(state_dot_legend(&row_state));
        ui.add_space(2.0);

        let name_text = match resolved.likelihood {
            ServerLikelihood::NotServer => egui::RichText::new(&resolved.canonical_name)
                .weak()
                .italics(),
            _ => egui::RichText::new(&resolved.canonical_name).strong(),
        };
        let entry_hover = entry_points_hover(resolved);
        ui.add(egui::Label::new(name_text).truncate())
            .on_hover_text(&entry_hover);

        if resolved.entry_points.len() > 1 {
            ui.label(
                egui::RichText::new(format!("▸{}", resolved.entry_points.len()))
                    .small()
                    .weak(),
            )
            .on_hover_text(&entry_hover);
        }
        // Port lives only inside the state pill now — no standalone mono.
        ui.separator();
        render_service_state_inline(ui, &row_state);

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            render_service_actions_inline(ui, w, resolved, &row_state, snap, pending);
        });
    });
}

fn entry_points_hover(resolved: &ResolvedService) -> String {
    let mut s = String::new();
    for (i, ep) in resolved.entry_points.iter().enumerate() {
        let prefix = if i == 0 { "▸ " } else { "  " };
        s.push_str(&format!("{prefix}{} — {}\n", ep.name, ep.command));
    }
    s.trim_end().to_string()
}

fn render_service_state_inline(ui: &mut egui::Ui, row_state: &RowState) {
    match row_state {
        RowState::Running { started_at, .. } => {
            let txt = format!("running · {}", uptime_short(*started_at));
            status_pill(ui, StatusKind::Good, txt, Some("started by Hive"));
        }
        RowState::ExternalLive { port, .. } => {
            status_pill(
                ui,
                StatusKind::Info,
                format!("live (external) · :{port}"),
                Some(
                    "a process bound to this command's expected port is already running \
                     (not started by Hive) — see listener row below",
                ),
            );
        }
        RowState::Blocked {
            port, holder_label, ..
        } => {
            status_pill(
                ui,
                StatusKind::Danger,
                format!("blocked · :{port} held by {holder_label}"),
                Some("another listener is already bound — Start would fail with EADDRINUSE"),
            );
        }
        RowState::Idle => {
            ui.label(egui::RichText::new("idle").weak());
        }
    }
}

fn render_service_actions_inline(
    ui: &mut egui::Ui,
    w: &WorktreeRef,
    resolved: &ResolvedService,
    row_state: &RowState,
    snap: &Snapshot,
    pending: &mut Pending,
) {
    // Action button labels are short — the port lives in the state pill,
    // not on every button. Hover gives port + tooltip context.
    let primary = resolved.primary_entry_point();
    match row_state {
        RowState::Running { pgid, .. } => {
            let ports = snap.ports_by_pgid.get(pgid).cloned().unwrap_or_default();
            let port_hover = ports
                .first()
                .map(|p| format!("Open :{p} in browser"))
                .unwrap_or_else(|| "no port bound yet".into());
            if ui
                .add_enabled(!ports.is_empty(), egui::Button::new("Open"))
                .on_hover_text(port_hover)
                .clicked()
            {
                if let Some(p) = ports.first() {
                    pending.open = Some(*p);
                }
            }
            if ui
                .add(egui::Button::new("Stop").fill(theme::DANGER))
                .clicked()
            {
                pending.stop = Some((*pgid, primary.name.clone()));
            }
        }
        RowState::ExternalLive { port, .. } => {
            // The listener row backing this port is folded into THIS row.
            // Kill targets the port-holder's pgid via the by_port index.
            if ui
                .button("Open")
                .on_hover_text(format!("Open :{port} in browser"))
                .clicked()
            {
                pending.open = Some(*port);
            }
            if let Some(al) = snap.by_port.get(port) {
                if ui
                    .add(egui::Button::new("Kill").fill(theme::DANGER))
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
        RowState::Blocked { .. } => {
            ui.add_enabled(false, egui::Button::new("Start"))
                .on_disabled_hover_text(
                    "port already held; stop or kill the blocking process first",
                );
        }
        RowState::Idle => {
            if ui.button("Start").clicked() {
                pending.start = Some((w.path.clone(), primary.clone()));
            }
        }
    }
}

fn state_dot_color(row_state: &RowState) -> egui::Color32 {
    match row_state {
        RowState::Running { .. } => theme::GREEN,
        RowState::ExternalLive { .. } => theme::SKY,
        RowState::Blocked { .. } => theme::WARN_ORANGE,
        RowState::Idle => egui::Color32::GRAY,
    }
}

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

// ── listeners strip ─────────────────────────────────────────────────────

/// Listener strip — only renders rows that AREN'T already represented by
/// a service row in this worktree. When a listener's port matches the
/// `expected_port` of a visible service, that service row already shows
/// the state pill + (for external) the Kill button — so a separate
/// listener row would be double-counting.
fn render_listeners_strip(
    ui: &mut egui::Ui,
    listeners: &[AttributedListener],
    service_ports: &std::collections::HashSet<u16>,
    snap: &Snapshot,
    pending: &mut Pending,
) {
    let visible: Vec<&AttributedListener> = listeners
        .iter()
        .filter(|l| !service_ports.contains(&l.listener.port))
        .filter(|l| listener_matches(l, &snap.filter_lc))
        .collect();
    if visible.is_empty() {
        return;
    }
    // No sub-label — the Kill verb identifies the strip.
    ui.indent("lstn_indent", |ui| {
        for l in visible {
            render_listener_line(ui, l, pending);
        }
    });
}

fn listener_matches(l: &AttributedListener, filter_lc: &str) -> bool {
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

fn render_listener_line(ui: &mut egui::Ui, l: &AttributedListener, pending: &mut Pending) {
    ui.horizontal(|ui| {
        theme::painted_dot(ui, theme::GREEN);
        ui.add_space(2.0);
        mono_label(ui, &format!(":{}", l.listener.port), None);
        ui.add(egui::Label::new(&l.listener.command_name).truncate())
            .on_hover_text(format!(
                "{}\npid {} · pgid {}",
                l.listener.command_name, l.listener.pid, l.listener.pgid
            ));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .add(egui::Button::new("Kill").fill(theme::DANGER))
                .clicked()
            {
                pending.kill = Some(l.listener.pgid);
            }
            if let Some(p) = &l.listener.cwd {
                path_cell(ui, p);
            }
        });
    });
}

// ── unattributed card ───────────────────────────────────────────────────

fn render_unattributed_card(ui: &mut egui::Ui, list: &[AttributedListener], pending: &mut Pending) {
    let id = ui.make_persistent_id("unattr_card");
    let state = CollapsingState::load_with_default_open(ui.ctx(), id, false);
    egui::Frame::group(ui.style())
        .inner_margin(egui::Margin::same(10.0))
        .show(ui, |ui| {
            state
                .show_header(ui, |ui| {
                    theme::painted_dot_hollow(ui, egui::Color32::GRAY);
                    ui.add_space(2.0);
                    ui.label(egui::RichText::new("Unattributed listeners").strong());
                    ui.label(egui::RichText::new(format!("({})", list.len())).weak());
                })
                .body(|ui| {
                    for l in list {
                        render_listener_line(ui, l, pending);
                    }
                });
        });
}

// ── filter (worktree-level) ─────────────────────────────────────────────

fn worktree_matches(w: &WorktreeRef, snap: &Snapshot, filter_lc: &str) -> bool {
    if filter_lc.is_empty() {
        return true;
    }
    if w.repo_name.to_lowercase().contains(filter_lc)
        || w.branch
            .as_deref()
            .unwrap_or("")
            .to_lowercase()
            .contains(filter_lc)
        || w.path.to_string_lossy().to_lowercase().contains(filter_lc)
    {
        return true;
    }
    if let Some(svcs) = snap.services.get(&w.path) {
        if svcs.iter().any(|s| service_matches_filter(s, w, filter_lc)) {
            return true;
        }
    }
    if let Some(list) = snap.listeners_by_wt.get(&w.path) {
        if list.iter().any(|l| listener_matches(l, filter_lc)) {
            return true;
        }
    }
    false
}

// ── kill-all confirm modal + accessor for top bar ───────────────────────

pub fn unique_pgids_in_filter(app: &HiveApp) -> Vec<i32> {
    Snapshot::collect(app).unique_pgids_in_filter()
}

fn render_kill_all_modal(app: &mut HiveApp, ctx: &egui::Context) {
    if !app.confirm_kill_all {
        return;
    }
    let pgids = Snapshot::collect(app).unique_pgids_in_filter();
    let mut open = true;
    let mut do_confirm = false;
    let mut do_cancel = false;
    let n = pgids.len();
    egui::Window::new("Confirm kill all")
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            ui.label(format!(
                "Send SIGTERM (then SIGKILL after 3s) to {n} unique process group{} in \
                 the current filter?",
                if n == 1 { "" } else { "s" }
            ));
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui
                    .add(
                        egui::Button::new(
                            egui::RichText::new("Confirm").color(egui::Color32::WHITE),
                        )
                        .fill(theme::DANGER),
                    )
                    .clicked()
                {
                    do_confirm = true;
                }
                if ui.button("Cancel").clicked() {
                    do_cancel = true;
                }
            });
        });
    if do_confirm {
        app.spawn_kill_many(pgids, ctx);
        app.confirm_kill_all = false;
    } else if do_cancel || !open {
        app.confirm_kill_all = false;
    }
}
