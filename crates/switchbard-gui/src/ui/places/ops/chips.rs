//! The Services/Listening cells' per-item chip rendering and filter
//! predicates — split out of `row.rs` (TASK-100 medic pass) so that module
//! stays about *row assembly* (which cells a worktree gets, in what order,
//! capped how) rather than growing indefinitely with *cell content* logic
//! too. See `power-of-10-overrides.md`'s "Known debt" note on `row.rs`'s
//! size for why this split matters: new Ops work should carve toward smaller
//! modules, not pile onto the table-rendering file further.
//!
//! Two families live here: one service chip ("▶ gui" idle, "■ vite"
//! running — `render_service_chip`) with the tiered Open-button port
//! resolution behind it (`open_port_for_running`'s four tiers, unchanged
//! from the retired swimlane view), and one listening chip (":5173" plus
//! open/kill — `render_listening_chip`). `row.rs` calls into both through
//! `render_capped_chip_row`, which stays there — it's a generic per-cell
//! layout primitive, not chip-specific.

use std::collections::BTreeSet;
use std::path::Path;
use std::time::Instant;

use eframe::egui;

use crate::ui::components::mono_label;
use crate::ui::theme;
use switchbard_core::{
    default_port_for_service, AttributedListener, ResolvedService, ServerLikelihood, WorktreeRef,
};

use super::{is_containerized, Pending, Snapshot};
use crate::runtime::RowState;

pub(super) fn should_skip_service(
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
pub(super) fn render_service_chip(
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
                "{name} - blocked: :{port} already held by {holder_label}"
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

pub(super) fn render_listening_chip(
    ui: &mut egui::Ui,
    l: &AttributedListener,
    pending: &mut Pending,
) {
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
