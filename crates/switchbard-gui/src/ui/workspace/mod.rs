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
use crate::runtime::worktree_names::worktree_display_name;
use crate::runtime::{
    dispatch_run_holds_worktree, removal_facts, ActiveRun, ActivityLevel, ConfirmRemoveWorktree,
    LandingEntry, RowState, WorktreeMeta, WorktreeSizeEntry,
};
use crate::ui::components::{
    branch_label, mono_label, path_cell, status_pill, weak_dots, Chip, StatusKind,
};
use crate::ui::theme;
use eframe::egui::{self, collapsing_header::CollapsingState};
use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use switchbard_core::{
    default_port_for_service, humanize_age, resolve, AttachedProcesses, AttributedListener,
    CheckOutcome, DetectedService, DriftProbe, RemovalCheck, RemovalIntent, RemovalSafety,
    RemovalVerdict, Repo, ResolvedService, ServerLikelihood, ServiceSource, TrunkDivergence,
    WorktreeRef,
};

mod bulk_remove;
pub mod create_worktree;
pub mod landing;
pub mod rename_worktree;
pub mod staleness;
pub mod tooltips;
use staleness::StalenessFilter;
use tooltips::{activity_tooltip, dirty_tooltip, ref_drift_tooltip, trunk_tooltip};

/// Actions queued during the walk; applied after the central panel
/// closure exits so we don't double-borrow `app`.
#[derive(Default)]
struct Pending {
    start: Option<(PathBuf, DetectedService)>,
    stop: Option<(i32, String)>,
    open: Option<u16>,
    kill: Option<i32>,
    open_create_worktree: Option<Repo>,
    open_rename_worktree: Option<(Repo, WorktreeRef)>,
    /// (repo_name, worktree_path, branch) — `apply_pending` resolves repo_name
    /// to a path via `app.config.repos` and opens the confirm dialog.
    open_remove_worktree: Option<(String, PathBuf, Option<String>)>,
}

pub fn render(app: &mut HiveApp, ui: &mut egui::Ui) {
    let ctx = &ui.ctx().clone();
    let snap = Snapshot::collect(app);
    let mut pending = Pending::default();

    egui::CentralPanel::default().show(ui, |ui| {
        render_summary(ui, &snap);
        ui.add_space(4.0);
        staleness::render_filter_bar(ui, app, &snap);
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
                    if wts.is_empty() || !wts.iter().any(|w| worktree_visible(w, &snap)) {
                        continue;
                    }
                    render_repo_card(ui, repo, &wts, &snap, app, &mut pending);
                    ui.add_space(8.0);
                }
                if !snap.show_only_managed && !snap.unattributed.is_empty() {
                    render_unattributed_card(ui, &snap.unattributed, &mut pending);
                }
            });
    });

    apply_pending(app, ui, pending);
    render_kill_all_modal(app, ui);
    render_remove_worktree_modal(app, ui);
    bulk_remove::render_modal(app, ui);
    create_worktree::render_modal(app, ctx);
    rename_worktree::render_modal(app, ctx);
}

fn apply_pending(app: &mut HiveApp, ui: &mut egui::Ui, p: Pending) {
    let ctx = &ui.ctx().clone();
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
    if let Some(repo) = p.open_create_worktree {
        app.open_create_worktree(repo);
    }
    if let Some((repo, worktree)) = p.open_rename_worktree {
        app.open_rename_worktree(repo, worktree);
    }
    if let Some((repo_name, wt_path, branch)) = p.open_remove_worktree {
        if let Some(repo_path) = app
            .config
            .repos
            .iter()
            .find(|r| r.name == repo_name)
            .map(|r| r.path.clone())
        {
            app.open_remove_worktree_confirm(repo_path, wt_path, branch);
        }
    }
}

// ── snapshot ──────────────────────────────────────────────────────────────

struct Snapshot {
    repos: Vec<Repo>,
    worktrees: Vec<WorktreeRef>,
    meta: HashMap<PathBuf, WorktreeMeta>,
    /// TASK-41: on-disk size cache, refreshed on its own cadence — see
    /// `WorktreeSizeEntry`'s doc.
    sizes: HashMap<PathBuf, WorktreeSizeEntry>,
    /// feat/landing-stage: the shared cache itself, not a per-frame clone of
    /// it. Unlike `sizes`/`meta` above, most worktrees have no unlanded work
    /// and so never enter this map at all, but the ones that do carry a PR
    /// URL string — cheap per-entry, wasteful to clone in bulk every frame
    /// for rows that won't render it. `render_worktree_row` looks up one
    /// path at a time instead (`landing.lock().unwrap().get(path).cloned()`).
    landing: Arc<Mutex<HashMap<PathBuf, LandingEntry>>>,
    services: HashMap<PathBuf, Vec<ResolvedService>>,
    listeners_by_wt: HashMap<PathBuf, Vec<AttributedListener>>,
    unattributed: Vec<AttributedListener>,
    active_runs: HashMap<i32, ActiveRun>,
    /// How many dispatch agent runs still hold each worktree. A count rather
    /// than the runs themselves: `HiveApp::dispatch_runs` is rebuilt per
    /// frame and cloning every `DispatchRun` into the snapshot would put a
    /// pile of `PathBuf`s on the render path for a number the row needs one
    /// integer of. Only worktrees with a live run appear at all.
    dispatch_holds_by_wt: HashMap<PathBuf, usize>,
    by_port: HashMap<u16, AttributedListener>,
    ports_by_pgid: HashMap<i32, Vec<u16>>,
    filter_lc: String,
    show_only_managed: bool,
    raw_detected_total: usize,
    staleness_filter: StalenessFilter,
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

        let mut dispatch_holds_by_wt: HashMap<PathBuf, usize> = HashMap::new();
        for run in app.dispatch_runs.lock().unwrap().values() {
            if dispatch_run_holds_worktree(&run.liveness) {
                *dispatch_holds_by_wt
                    .entry(run.worktree_path.clone())
                    .or_default() += 1;
            }
        }

        Self {
            repos: app.repos_snapshot(),
            worktrees: app.worktrees_snapshot(),
            meta,
            sizes: app.sizes.lock().unwrap().clone(),
            landing: app.landing.clone(),
            services,
            listeners_by_wt,
            unattributed,
            active_runs,
            dispatch_holds_by_wt,
            by_port,
            ports_by_pgid,
            filter_lc: app.filter.to_lowercase(),
            show_only_managed: app.show_only_managed,
            raw_detected_total,
            staleness_filter: app.staleness_filter,
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
    ui.label(egui::RichText::new(s).color(theme::weak_text()));
}

// ── repo card ────────────────────────────────────────────────────────────

fn render_repo_card(
    ui: &mut egui::Ui,
    repo: &Repo,
    wts: &[&WorktreeRef],
    snap: &Snapshot,
    app: &mut HiveApp,
    pending: &mut Pending,
) {
    let mut listening = 0usize;
    let mut dirty = 0usize;
    let mut unlanded_worktrees = 0usize;
    let mut remote_attention = 0usize;
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
            if has_unlanded_work(&m.trunk) {
                unlanded_worktrees += 1;
            }
            if drift_needs_attention(&m.remote_drift) {
                remote_attention += 1;
            }
        }
    }

    egui::Frame::group(ui.style())
        .inner_margin(egui::Margin::same(10))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                if listening > 0 {
                    theme::painted_dot_pulse(ui, theme::green(), listening);
                } else {
                    theme::painted_dot(ui, theme::idle_dot());
                }
                ui.add_space(2.0);
                ui.heading(&repo.name);
                ui.label(
                    egui::RichText::new(format!("{} wt", wts.len())).color(theme::weak_text()),
                );
                // Chips quiet down: dirty/drifted only when the repo has more
                // worktrees than the eye can summarize at a glance. Listener
                // count is on the dot's pulse, no chip needed.
                if wts.len() > 3 {
                    let chip_storage = build_chips(dirty, unlanded_worktrees, remote_attention);
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
                    if ui
                        .button("+ Worktree")
                        .on_hover_text("Create worktree")
                        .clicked()
                    {
                        pending.open_create_worktree = Some(repo.clone());
                    }
                    ui.add_space(6.0);
                    ui.label(
                        egui::RichText::new(repo.path.display().to_string())
                            .color(theme::weak_text())
                            .small(),
                    );
                });
            });
            ui.add_space(4.0);

            for w in wts {
                if !worktree_visible(w, snap) {
                    continue;
                }
                let is_primary = w.path == repo.path;
                ui.push_id(format!("wt_{}", w.path.display()), |ui| {
                    render_worktree_row(ui, repo, w, is_primary, snap, app, pending);
                });
            }
        });
}

fn build_chips(
    dirty: usize,
    unlanded_worktrees: usize,
    remote_attention: usize,
) -> Vec<(egui::Color32, String)> {
    let mut chips = Vec::new();
    if dirty > 0 {
        chips.push((theme::amber(), format!("{dirty} dirty")));
    }
    if unlanded_worktrees > 0 {
        chips.push((theme::lavender(), format!("{unlanded_worktrees} unlanded")));
    }
    if remote_attention > 0 {
        chips.push((theme::sky(), format!("{remote_attention} remote")));
    }
    chips
}

/// Does this worktree hold work the trunk doesn't?
///
/// Content, not ancestry: a rebase-merged branch is "ahead" of the trunk by
/// ancestry and holds nothing at risk, so counting it here made the row's
/// lavender dot and the repo card's "N vs main" chip disagree with the row's
/// own `remove ok` badge.
fn has_unlanded_work(trunk: &Option<TrunkDivergence>) -> bool {
    trunk.as_ref().is_some_and(|t| t.unlanded > 0)
}

fn drift_needs_attention(probe: &Option<DriftProbe>) -> bool {
    probe.as_ref().is_some_and(DriftProbe::needs_attention)
}

// ── worktree row ─────────────────────────────────────────────────────────

fn render_worktree_row(
    ui: &mut egui::Ui,
    repo: &Repo,
    w: &WorktreeRef,
    is_primary: bool,
    snap: &Snapshot,
    app: &mut HiveApp,
    pending: &mut Pending,
) {
    let default_meta;
    let m = if let Some(meta) = snap.meta.get(&w.path) {
        meta
    } else {
        default_meta = WorktreeMeta::default();
        &default_meta
    };
    // feat/landing-stage: one point lookup into the shared cache, not a
    // per-frame clone of the whole map (see `Snapshot::landing`'s doc) —
    // and the only place this row's chip state gets decided, so the trailing
    // cluster below just paints what this function already worked out.
    let landing_view = landing::landing_chip(
        has_unlanded_work(&m.trunk),
        w.branch.is_some(),
        snap.landing.lock().unwrap().get(&w.path),
    );
    let listeners: &[AttributedListener] = snap
        .listeners_by_wt
        .get(&w.path)
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    let svcs: &[ResolvedService] = snap.services.get(&w.path).map(Vec::as_slice).unwrap_or(&[]);
    let any_running_or_external = svcs.iter().any(|resolved| {
        let run = snap.run_for_resolved(&w.path, resolved);
        let c = is_containerized(resolved);
        matches!(
            RowState::compute(resolved.expected_port, &w.path, run, &snap.by_port, c),
            RowState::Running { .. } | RowState::ExternalLive { .. }
        )
    });
    let noteworthy = is_noteworthy(listeners, m, any_running_or_external);
    let default_open = noteworthy || !snap.filter_lc.is_empty();

    // Both primary and linked worktrees get the same inner margin so
    // their row heights stay consistent; only the fill differs. This
    // keeps the swimlane visually rhythmic when scanning down the
    // list.
    //
    // Selection wins over the primary tint, and the two can never actually
    // collide: primaries render no bulk checkbox and are dropped from the
    // candidate list, so a primary is never selected. The ordering is written
    // out anyway rather than left implicit, because "selected" is the state
    // the user just acted on and it should never be the one that loses.
    //
    // Highlighting the row itself, not just the checkbox: "Select all merged
    // + clean" ticks an arbitrary subset of a long list, and a 14px checkbox
    // is not an answer to "what did that just take?" at a glance.
    let selected = app.bulk_selected_worktrees.contains(&w.path);
    let mut frame = egui::Frame::NONE.inner_margin(egui::Margin::symmetric(4, 1));
    if selected {
        frame = frame
            .fill(theme::selected_row_tint())
            .stroke(theme::selected_row_stroke());
    } else if is_primary {
        frame = frame.fill(theme::primary_worktree_tint());
    }
    // Clicking anywhere in the row's header selects it for bulk removal.
    //
    // The gesture lives on the header's own `Ui`, made click-sensing at
    // creation via `UiBuilder::sense`. That ordering is the whole trick, and
    // it is not incidental:
    //
    //   - egui resolves an overlapping click to the *last-registered*
    //     click-sensing widget (`hit_test_on_close`). A row rect registered
    //     after its contents therefore swallows every button in the row.
    //   - A `Ui` sensed at creation registers *before* its children, so
    //     Rename, the trash button and the checkbox all still win their own
    //     clicks. `Ui::remember_min_rect` re-registers it at the end only to
    //     correct the rect, with `move_to_top: false`, so it keeps its early
    //     position.
    //   - Hover-only labels sitting on top cannot absorb the click: the hit
    //     test filters candidates by `Sense::senses_click`, so a plain
    //     `Label` is never a click target in the first place. An earlier pass
    //     assumed otherwise and put the gesture on the name alone, which left
    //     the rest of the row dead.
    //
    // Scoped to the header, not the frame, so clicking inside an expanded
    // row's body (service and listener actions) does not select. The
    // expand/collapse triangle is `show_header`'s own widget, laid out to the
    // left of this `Ui` and outside its rect, so it keeps toggling.
    //
    // `row_click_selects_but_buttons_still_win` and its neighbours in
    // `bulk_remove_worktrees.rs` pin each half.
    //
    // Primaries are excluded — they render no checkbox and are dropped from
    // the candidate list, so selecting one offers a selection that evaporates.
    let mut row_clicked = false;
    frame.show(ui, |ui| {
        let id = ui.make_persistent_id(format!("wt_row_{}", w.path.display()));
        let state = CollapsingState::load_with_default_open(ui.ctx(), id, default_open);
        app.perf_count_worktree_row(state.is_open(), svcs.len(), listeners.len());
        state
            .show_header(ui, |ui| {
                let row =
                    ui.scope_builder(egui::UiBuilder::new().sense(egui::Sense::click()), |ui| {
                        // Claim the full remaining width so the dead space
                        // between the two clusters — the row's largest target
                        // — is part of the gesture, not a hole in it.
                        ui.set_min_width(ui.available_width());
                        // Labels are selectable by default, and a selectable
                        // `Label` senses click *and* drag (egui's
                        // `Label::layout_in_ui`) — so every name and chip in
                        // the row would register after this `Ui` and win the
                        // click, leaving only the gaps between them clickable.
                        // Row-select and drag-to-select-text want the same
                        // gesture on the same pixels; the row wins, because
                        // selecting an 8-char SHA or the word "clean" is not
                        // a thing anyone comes here to do. Tooltips still
                        // carry the full values.
                        ui.style_mut().interaction.selectable_labels = false;
                        let display_name = worktree_display_name(&app.config, repo, w);
                        let summary = WorktreeSummary {
                            worktree: w,
                            display_name: &display_name,
                            is_primary,
                            meta: m,
                            listener_count: listeners.len(),
                            services: svcs,
                            size: snap.sizes.get(&w.path),
                        };
                        render_worktree_summary_line(ui, summary, snap);
                        render_worktree_row_trailing(
                            ui,
                            repo,
                            w,
                            is_primary,
                            pending,
                            &mut app.bulk_selected_worktrees,
                            landing_view,
                        );
                    });
                if !is_primary {
                    // The only standing hint that the whole row is a target.
                    // A row-wide tooltip would fire over every chip that has
                    // one of its own, so the cursor carries it instead.
                    row_clicked = row
                        .response
                        .on_hover_cursor(egui::CursorIcon::PointingHand)
                        .clicked();
                }
            })
            .body(|ui| {
                ui.add_space(2.0);
                render_branch_inline(ui, w);
                let service_ports: std::collections::HashSet<u16> =
                    svcs.iter().filter_map(|s| s.expected_port).collect();
                if !svcs.is_empty() {
                    render_services_strip(ui, w, svcs, snap, app.show_non_servers, pending);
                }
                if !listeners.is_empty() {
                    render_listeners_strip(ui, listeners, &service_ports, snap, pending);
                }
                if svcs.is_empty() && listeners.is_empty() {
                    ui.label(
                        egui::RichText::new("nothing detected here").color(theme::weak_text()),
                    );
                }
                ui.add_space(4.0);
            });
    });

    if row_clicked && !app.bulk_selected_worktrees.remove(&w.path) {
        app.bulk_selected_worktrees.insert(w.path.clone());
    }
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
    if has_unlanded_work(&m.trunk) || drift_needs_attention(&m.remote_drift) {
        return true;
    }
    if let Some(act) = m.activity() {
        return matches!(act.level, ActivityLevel::Burst | ActivityLevel::Active);
    }
    false
}

struct WorktreeSummary<'a> {
    worktree: &'a WorktreeRef,
    display_name: &'a str,
    is_primary: bool,
    meta: &'a WorktreeMeta,
    listener_count: usize,
    services: &'a [ResolvedService],
    /// TASK-41: on-disk size, `None` while `workers::spawn_size` hasn't
    /// reached this worktree yet.
    size: Option<&'a WorktreeSizeEntry>,
}

fn render_worktree_summary_line(ui: &mut egui::Ui, summary: WorktreeSummary<'_>, snap: &Snapshot) {
    let (dot_color, pulse_count) = headline_dot(
        summary.services,
        summary.worktree,
        snap,
        summary.listener_count,
        summary.meta,
    );
    if pulse_count > 0 {
        theme::painted_dot_pulse(ui, dot_color, pulse_count);
    } else {
        theme::painted_dot(ui, dot_color);
    }
    ui.add_space(2.0);
    // Plain label: the select gesture belongs to the row's `Ui`, which senses
    // the whole header (see `render_worktree_row`). One authority for "what
    // selects this row", not two.
    ui.label(egui::RichText::new(summary.display_name).strong());
    // Branch name lives in the expanded body (`render_branch_inline`), not
    // here: long branches pushed the left cluster into the right-aligned
    // Rename/trash actions and overlapped them.
    // Health zone: dirty appears only when dirty; drift only when non-zero;
    // listener count is on the dot tooltip already (no inline tag).
    render_health_inline(ui, summary.meta);
    // TASK-41: Merged/NoUpstream/Live badge + on-disk size, right after the
    // existing dirty/drift health pills — same "one inline zone" pattern.
    staleness::render_staleness_badge(ui, summary.meta);
    staleness::render_size_label(ui, summary.size);
    render_activity_inline(ui, summary.meta);
    render_delete_safety_inline(
        ui,
        summary.is_primary,
        summary.meta,
        attached_processes(snap, &summary.worktree.path, summary.listener_count),
    );
}

/// Branch name for the expanded body. Kept off the collapsed header so a long
/// branch can't crowd the right-aligned Rename/trash actions; here it has the
/// full row width and truncates with a hover-to-reveal tooltip.
fn render_branch_inline(ui: &mut egui::Ui, w: &WorktreeRef) {
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new("branch")
                .small()
                .color(theme::weak_text()),
        );
        branch_label(ui, w.branch.as_deref());
    });
}

/// Right-aligned cluster on the worktree row header: the landing-stage chip
/// on the far right (feat/landing-stage — the head SHA's old spot, see
/// below), plus a small remove-worktree affordance (hidden on the primary
/// worktree, which can't be removed via `git worktree remove`).
///
/// Split from the summary line so each function's arg count stays tame;
/// also lets us add more right-side affordances later without touching
/// the left-side layout.
fn render_worktree_row_trailing(
    ui: &mut egui::Ui,
    repo: &Repo,
    w: &WorktreeRef,
    is_primary: bool,
    pending: &mut Pending,
    bulk_selected: &mut BTreeSet<PathBuf>,
    landing_view: Option<landing::LandingChipView>,
) {
    // No head SHA here any more. It was the row's only element that answered
    // no question the row is for — every other one maps to an action (dirty →
    // commit, unlanded → land, size → reclaim, the badges → can this go) —
    // and its one real use, copy-paste, had already been taken away by the
    // row-click gesture, which turns off `selectable_labels` for this whole
    // header. A string you can neither act on nor copy is decoration, and it
    // was occupying the far-right slot the landing-stage chip now fills.
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        // feat/landing-stage: first widget added in a right-to-left layout
        // paints furthest right — exactly where the head SHA used to sit.
        // Renders nothing at all for "no unlanded work" or "detached HEAD"
        // (see `landing::landing_chip`'s doc), so a boring row stays boring.
        landing::render_landing_chip(ui, landing_view);
        if ui
            .small_button("Rename")
            .on_hover_text("Rename Switchbard label")
            .clicked()
        {
            pending.open_rename_worktree = Some((repo.clone(), w.clone()));
        }
        if !is_primary {
            ui.add_space(2.0);
            let resp = theme::painted_trash_button(ui);
            if resp.on_hover_text("Remove worktree…").clicked() {
                pending.open_remove_worktree =
                    Some((w.repo_name.clone(), w.path.clone(), w.branch.clone()));
            }
        }
        // TASK-41: bulk-select checkbox, closest to the row content so it
        // reads as "part of this row" rather than another trailing action.
        ui.add_space(4.0);
        bulk_remove::render_select_checkbox(ui, w, is_primary, bulk_selected);
    });
}

fn headline_dot(
    svcs: &[ResolvedService],
    w: &WorktreeRef,
    snap: &Snapshot,
    listener_count: usize,
    m: &WorktreeMeta,
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
        return (theme::green(), listener_count.max(running));
    }
    if external > 0 {
        return (theme::sky(), 0);
    }
    if m.is_dirty() == Some(true) {
        return (theme::amber(), 0);
    }
    if has_unlanded_work(&m.trunk) {
        return (theme::lavender(), 0);
    }
    if drift_needs_attention(&m.remote_drift) {
        return (theme::sky(), 0);
    }
    (theme::idle_dot(), 0)
}

/// One inline "health" zone: dirty + drift on a single line. Both fields
/// are explicit so a linked worktree never looks unprobed just because it is
/// clean or in sync.
fn render_health_inline(ui: &mut egui::Ui, m: &WorktreeMeta) {
    render_dirty_inline(ui, m);
    render_trunk_inline(ui, m.trunk.as_ref(), m.trunk_detail.as_ref());
    render_drift_inline(
        ui,
        "remote",
        m.remote_drift.as_ref(),
        m.remote_drift_detail.as_ref(),
        m.fetch_unix,
        theme::sky(),
    );
}

fn render_dirty_inline(ui: &mut egui::Ui, m: &WorktreeMeta) {
    match m.is_dirty() {
        Some(true) => {
            let tip = dirty_tooltip(m.dirty_files.as_deref().unwrap_or(&[]));
            status_pill(ui, StatusKind::Warn, "dirty", Some(&tip));
        }
        Some(false) => {
            status_pill(
                ui,
                StatusKind::Neutral,
                "clean",
                Some("No uncommitted changes"),
            );
        }
        None => {
            ui.label(egui::RichText::new("dirty ...").color(theme::weak_text()))
                .on_hover_text("Dirty probe pending or failed");
        }
    }
}

/// The trunk chip: how much of this worktree's work isn't upstream yet.
///
/// Reads `+N/-M` like the remote chip beside it, but `N` is unlanded commits
/// rather than commits ahead by ancestry — see `TrunkDivergence` for why the
/// two are not the same question. `N == 0` therefore means "nothing here would
/// be lost", which is exactly what the row's `remove ok` badge is claiming a
/// few pixels to the right.
fn render_trunk_inline(
    ui: &mut egui::Ui,
    divergence: Option<&TrunkDivergence>,
    detail: Option<&switchbard_core::TrunkDetail>,
) {
    let Some(d) = divergence else {
        ui.label(egui::RichText::new("trunk ...").color(theme::weak_text()))
            .on_hover_text("Trunk comparison pending, or no local main/master to compare against");
        return;
    };
    let text = format!("{} +{}/-{}", d.base, d.unlanded, d.behind);
    let tip = trunk_tooltip(d, detail);
    if d.unlanded + d.behind > 0 {
        mono_label(ui, &text, Some(theme::lavender())).on_hover_text(tip);
    } else {
        ui.label(
            egui::RichText::new(text)
                .monospace()
                .color(theme::weak_text()),
        )
        .on_hover_text(tip);
    }
}

fn render_drift_inline(
    ui: &mut egui::Ui,
    label: &str,
    probe: Option<&DriftProbe>,
    detail: Option<&switchbard_core::DriftDetail>,
    fetch_unix: Option<u64>,
    drift_color: egui::Color32,
) {
    let Some(probe) = probe else {
        ui.label(egui::RichText::new(format!("{label} ...")).color(theme::weak_text()))
            .on_hover_text(format!("{label} comparison pending or failed"));
        return;
    };

    let tip = ref_drift_tooltip(label, probe, detail, fetch_unix);
    match probe {
        DriftProbe::Ready { ahead, behind, .. } => {
            let text = format!("{label} +{ahead}/-{behind}");
            if ahead + behind > 0 {
                mono_label(ui, &text, Some(drift_color)).on_hover_text(tip);
            } else {
                ui.label(
                    egui::RichText::new(text)
                        .monospace()
                        .color(theme::weak_text()),
                )
                .on_hover_text(tip);
            }
        }
        DriftProbe::MissingBase { .. } => {
            status_pill(ui, StatusKind::Warn, format!("{label} missing"), Some(&tip));
        }
        DriftProbe::NoUpstream => {
            status_pill(ui, StatusKind::Warn, "no upstream", Some(&tip));
        }
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

/// The row's one-word answer to "can this go", plus the full check list on
/// hover. Every rule behind it lives in `switchbard_core::removal_safety`;
/// this function only picks a colour.
///
/// The intent is [`RemovalIntent::WorktreeAndBranch`] because that is what
/// the bulk sweep does - it defaults its "also delete branches" box on - and
/// a row that reads green while the sweep routes it to "needs review" is the
/// exact disagreement this whole module was collapsed to remove.
fn render_delete_safety_inline(
    ui: &mut egui::Ui,
    is_primary: bool,
    m: &WorktreeMeta,
    attached: AttachedProcesses,
) {
    ui.add_space(4.0);
    let facts = removal_facts(is_primary, m, attached);
    let safety = RemovalSafety::evaluate(&facts, RemovalIntent::WorktreeAndBranch);
    // Blocked is amber, not red. For a worktree someone is actively working
    // in, "you can't delete this" is the correct and expected state, not an
    // error - the old badge painted that case in danger red every time. Red
    // here would train the eye to ignore it.
    let kind = match safety.verdict() {
        RemovalVerdict::Primary | RemovalVerdict::Checking => StatusKind::Neutral,
        RemovalVerdict::Safe => StatusKind::Good,
        RemovalVerdict::Blocked => StatusKind::Warn,
    };
    status_pill(ui, kind, safety.headline(), Some(&safety.tooltip()));
}

/// Everything holding on to this worktree, from the three independent places
/// that know: the port scanner, this instance's started services, and the
/// dispatch run table.
///
/// The dispatch half is the one the old check could not see at all. A
/// dispatched agent writes into a worktree without necessarily listening on
/// any port and without having been started by `spawn_start`, so a run in
/// flight read as "nothing running here".
fn attached_processes(snap: &Snapshot, wt_path: &Path, listener_count: usize) -> AttachedProcesses {
    AttachedProcesses {
        listeners: listener_count,
        switchbard_runs: snap
            .active_runs
            .values()
            .filter(|run| run.worktree_path == wt_path)
            .count(),
        dispatch_runs: snap.dispatch_holds_by_wt.get(wt_path).copied().unwrap_or(0),
    }
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
                .color(theme::weak_text())
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
                    .color(theme::weak_text()),
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
            status_pill(ui, StatusKind::Good, txt, Some("started by Switchbard"));
        }
        RowState::ExternalLive { port, .. } => {
            status_pill(
                ui,
                StatusKind::Info,
                format!("live (external) · :{port}"),
                Some(
                    "a process bound to this command's expected port is already running \
                     (not started by Switchbard) — see listener row below",
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
            ui.label(egui::RichText::new("idle").color(theme::weak_text()));
        }
    }
}

/// Where the Open-button port came from. The tooltip surfaces this so the
/// user knows whether we're certain (Pgid) or making an educated guess
/// (KnownDefault).
#[derive(Debug, Clone, PartialEq, Eq)]
enum OpenPortSource {
    /// A listener whose pgid equals the run's pgid — best signal.
    Pgid,
    /// A listener attributed to this worktree, not claimed by any *other*
    /// active run. Common for JS dev servers that detach workers into a
    /// different process group than the one Switchbard launched.
    WorktreeClaim,
    /// The port declared on the command line (e.g. `--port 6006`). The
    /// process may not have bound it yet.
    Declared,
    /// Well-known default for the canonical service name (storybook → 6006,
    /// vite → 5173, …). Last-resort hint.
    KnownDefault,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct OpenPortHint {
    port: u16,
    source: OpenPortSource,
}

impl OpenPortHint {
    fn tooltip(&self) -> String {
        match self.source {
            OpenPortSource::Pgid => format!("Open :{} in browser", self.port),
            OpenPortSource::WorktreeClaim => format!(
                "Open :{} in browser (listener attributed to this worktree)",
                self.port
            ),
            OpenPortSource::Declared => format!(
                "Open :{} in browser (port declared on the command line — service may not have bound it yet)",
                self.port
            ),
            OpenPortSource::KnownDefault => format!(
                "Open :{} in browser (well-known default for this service — service may not have bound it yet)",
                self.port
            ),
        }
    }
}

/// Tiered resolver for the Open button on a Running row.
///
/// Switchbard launches a service under pgid `run_pgid`, but many dev toolchains
/// (Storybook, Vite, Webpack-dev-server, Next.js, Django auto-reload, Rails
/// puma cluster) detach worker processes into a *different* process group
/// before binding their TCP listener. The exact-pgid match misses those.
///
/// Tiers, from highest to lowest confidence:
///  - **Pgid**: a listener whose pgid equals `run_pgid`.
///  - **WorktreeClaim**: exactly one listener attributed to this worktree
///    that isn't claimed by another active run on this worktree.
///  - **Declared**: `resolved.expected_port` (a `--port` flag on the command).
///  - **KnownDefault**: conventional default for the canonical name.
///
/// Returns `None` only when every tier comes up empty.
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
        return Some(OpenPortHint {
            port,
            source: OpenPortSource::Pgid,
        });
    }

    if let Some(port) = unclaimed_worktree_listener_port(run_pgid, worktree_path, snap) {
        return Some(OpenPortHint {
            port,
            source: OpenPortSource::WorktreeClaim,
        });
    }

    if let Some(port) = resolved.expected_port {
        return Some(OpenPortHint {
            port,
            source: OpenPortSource::Declared,
        });
    }

    if let Some(port) = default_port_for_service(&resolved.canonical_name) {
        return Some(OpenPortHint {
            port,
            source: OpenPortSource::KnownDefault,
        });
    }

    None
}

/// Listener-by-worktree fallback. Returns a port iff *exactly one* listener
/// attributed to `worktree_path` has a pgid that's neither this run's pgid
/// nor any other active run's pgid on this worktree. Single-match is the
/// only safe call — if two unclaimed listeners are present we can't tell
/// which one belongs to this run.
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
            let hint = open_port_for_running(*pgid, &w.path, resolved, snap);
            let (enabled, hover) = match &hint {
                Some(h) => (true, h.tooltip()),
                None => (
                    false,
                    "no listener observed, no port declared, no default known for this service"
                        .to_string(),
                ),
            };
            let resp = ui.add_enabled(enabled, egui::Button::new("Open"));
            let resp = if enabled {
                resp.on_hover_text(hover)
            } else {
                resp.on_disabled_hover_text(hover)
            };
            if resp.clicked() {
                if let Some(h) = hint {
                    pending.open = Some(h.port);
                }
            }
            if ui.add(theme::danger_button("Stop")).clicked() {
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
                    .add(theme::danger_button("Kill"))
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
        RowState::Running { .. } => theme::green(),
        RowState::ExternalLive { .. } => theme::sky(),
        RowState::Blocked { .. } => theme::warn_orange(),
        RowState::Idle => theme::idle_dot(),
    }
}

fn state_dot_legend(row_state: &RowState) -> &'static str {
    match row_state {
        RowState::Running { .. } => "running — started by Switchbard",
        RowState::ExternalLive { .. } => {
            "live — running, but not started by Switchbard (existing terminal session, \
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
        theme::painted_dot(ui, theme::green());
        ui.add_space(2.0);
        mono_label(ui, &format!(":{}", l.listener.port), None);
        ui.add(egui::Label::new(&l.listener.command_name).truncate())
            .on_hover_text(format!(
                "{}\npid {} · pgid {}",
                l.listener.command_name, l.listener.pid, l.listener.pgid
            ));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.add(theme::danger_button("Kill")).clicked() {
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
        .inner_margin(egui::Margin::same(10))
        .show(ui, |ui| {
            state
                .show_header(ui, |ui| {
                    theme::painted_dot_hollow(ui, theme::idle_dot());
                    ui.add_space(2.0);
                    ui.label(egui::RichText::new("Unattributed listeners").strong());
                    ui.label(
                        egui::RichText::new(format!("({})", list.len())).color(theme::weak_text()),
                    );
                })
                .body(|ui| {
                    for l in list {
                        render_listener_line(ui, l, pending);
                    }
                });
        });
}

// ── filter (worktree-level) ─────────────────────────────────────────────

/// A worktree row renders iff it passes BOTH the freeform text filter and
/// the staleness filter chip (TASK-41) — the two are independent, ANDed
/// conditions, same as "only attributed listeners" + text filter already are
/// for the Servers view generally.
fn worktree_visible(w: &WorktreeRef, snap: &Snapshot) -> bool {
    worktree_matches(w, snap, &snap.filter_lc)
        && staleness::passes_staleness_filter(snap.staleness_filter, snap.meta.get(&w.path))
}

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
    let filter_lc = app.filter.to_lowercase();
    let show_only_managed = app.show_only_managed;
    let listeners = app.state.lock().unwrap().listeners.clone();
    let mut set: BTreeSet<i32> = BTreeSet::new();
    for listener in &listeners {
        if show_only_managed && listener.worktree_path.is_none() {
            continue;
        }
        if listener_matches(listener, &filter_lc) {
            set.insert(listener.listener.pgid);
        }
    }
    set.into_iter().collect()
}

/// Confirmation dialog for `git worktree remove`. Reads state from the
/// `Arc<Mutex<>>` once per frame; the worker thread driving the actual
/// removal can flip `busy`/`error` between frames so the dialog stays
/// responsive.
fn render_remove_worktree_modal(app: &mut HiveApp, ui: &mut egui::Ui) {
    let ctx = &ui.ctx().clone();
    let state = match app.confirm_remove_worktree.lock().unwrap().clone() {
        Some(s) => s,
        None => return,
    };

    let has_runs = !state.active_runs.is_empty();
    let is_dirty = !state.dirty_files.is_empty();
    let action_label = match (has_runs, is_dirty) {
        (false, false) => "Remove worktree",
        (true, false) => "Stop services and remove",
        (false, true) => "Discard changes and remove",
        (true, true) => "Stop services, discard changes, and remove",
    };

    let mut do_confirm = false;
    let mut do_cancel = false;
    // Local mirror of the checkbox; written back into the shared dialog state
    // after the frame so the worker reads the user's choice at confirm time.
    let mut delete_branch = state.delete_branch;

    egui::Window::new("Remove worktree")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            ui.set_max_width(540.0);
            ui.label(
                egui::RichText::new(format!(
                    "Remove worktree at {} ?",
                    state.worktree_path.display()
                ))
                .strong(),
            );
            // The same five lines the row badge shows on hover. Rendered
            // before the detail sections so the user reads the verdict first
            // and the enumeration second.
            render_shared_checks(ui, &state);
            render_branch_delete_section(ui, &state, &mut delete_branch);

            if has_runs {
                ui.add_space(6.0);
                ui.label(
                    egui::RichText::new(format!(
                        "⚠ {} service{} running here (started by switchbard):",
                        state.active_runs.len(),
                        if state.active_runs.len() == 1 {
                            ""
                        } else {
                            "s"
                        }
                    ))
                    .color(theme::amber()),
                );
                for run in &state.active_runs {
                    ui.label(format!("    {}    (pgid {})", run.service_name, run.pgid));
                }
            }

            if is_dirty {
                ui.add_space(6.0);
                ui.label(
                    egui::RichText::new(format!(
                        "⚠ {} uncommitted change{}:",
                        state.dirty_files.len(),
                        if state.dirty_files.len() == 1 {
                            ""
                        } else {
                            "s"
                        }
                    ))
                    .color(theme::amber()),
                );
                egui::ScrollArea::vertical()
                    .max_height(160.0)
                    .id_salt("remove_wt_dirty")
                    .show(ui, |ui| {
                        for f in &state.dirty_files {
                            ui.monospace(format!("    {}  {}", f.status, f.path.display()));
                        }
                    });
            }

            if let Some(err) = &state.error {
                ui.add_space(6.0);
                ui.colored_label(theme::danger(), err);
            }

            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.add_enabled_ui(!state.busy, |ui| {
                    if ui.button("Cancel").clicked() {
                        do_cancel = true;
                    }
                    let confirm_label = if delete_branch {
                        format!("{action_label} + delete branch")
                    } else {
                        action_label.to_string()
                    };
                    if ui.add(theme::danger_button(&confirm_label)).clicked() {
                        do_confirm = true;
                    }
                });
                if state.busy {
                    ui.add_space(4.0);
                    ui.spinner();
                    ui.label("removing…");
                }
            });
        });

    // Persist the checkbox toggle back into the shared dialog state before any
    // action runs, so the worker thread sees the user's final choice.
    if delete_branch != state.delete_branch {
        if let Some(s) = app.confirm_remove_worktree.lock().unwrap().as_mut() {
            s.delete_branch = delete_branch;
        }
    }

    if do_confirm {
        app.execute_remove_worktree(ctx);
    } else if do_cancel {
        app.cancel_remove_worktree_confirm();
    }
}

/// The shared `RemovalSafety` check list - the same sentences the Workspace
/// row shows on hover, so a user who saw "remove blocked" there reads the
/// identical reason here instead of a second opinion.
///
/// Deliberately [`RemovalIntent::WorktreeOnly`]: the `WorkLanded` line is
/// owned by the branch checkbox immediately below, which has to state it
/// anyway to explain what ticking the box would cost. Including it here too
/// would print the same sentence twice.
///
/// Failures are shown, not enforced. This dialog can force past a dirty tree
/// (that is what it is for), so its job is to make sure the user knows
/// exactly what they are forcing past.
fn render_shared_checks(ui: &mut egui::Ui, state: &ConfirmRemoveWorktree) {
    let safety = RemovalSafety::evaluate(&state.removal_facts, RemovalIntent::WorktreeOnly);
    ui.add_space(6.0);
    for check in safety.checks() {
        let color = match check.outcome {
            CheckOutcome::Pass => theme::weak_text(),
            CheckOutcome::Fail | CheckOutcome::Unknown => theme::amber(),
            CheckOutcome::Pending => theme::weak_text(),
        };
        ui.colored_label(
            color,
            format!("{} {}", check.outcome.marker(), check.detail),
        );
    }
}

/// The branch-cleanup row of the remove-worktree dialog:
///   - detached HEAD → nothing to offer;
///   - checked out elsewhere → git refuses, so show why instead of a checkbox;
///   - otherwise a checkbox, loud when the shared `WorkLanded` check says the
///     work has not landed.
///
/// "Has it landed" is read from the shared evaluation, never re-derived here.
/// `BranchDeleteAssessment` is consulted only for `is_blocked` - a different
/// question (would git accept the command) that is not a safety check.
fn render_branch_delete_section(
    ui: &mut egui::Ui,
    state: &ConfirmRemoveWorktree,
    delete_branch: &mut bool,
) {
    let Some(branch) = &state.branch else {
        return; // detached HEAD — no branch to delete
    };
    let Some(assessment) = &state.branch_assessment else {
        ui.label(format!("Branch '{branch}' will remain after removal."));
        return;
    };

    ui.add_space(6.0);

    if assessment.is_blocked() {
        *delete_branch = false;
        let where_ = assessment
            .other_checkouts
            .first()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "another worktree".to_string());
        ui.colored_label(
            theme::muted_text(),
            format!("Branch '{branch}' is checked out at {where_} — can't delete it here."),
        );
        return;
    }

    // One question, one answer: does the shared `WorkLanded` check pass?
    let landed = RemovalSafety::evaluate(&state.removal_facts, RemovalIntent::WorktreeAndBranch)
        .checks()
        .iter()
        .find(|c| c.check == RemovalCheck::WorkLanded)
        .map(|c| (c.outcome, c.detail.clone()));

    match landed {
        Some((CheckOutcome::Pass, detail)) => {
            ui.checkbox(
                delete_branch,
                format!("Also delete branch '{branch}' ({})", detail.to_lowercase()),
            );
        }
        Some((_, detail)) => {
            ui.checkbox(
                delete_branch,
                egui::RichText::new(format!("⚠ Force-delete branch '{branch}' - {detail}"))
                    .color(theme::danger()),
            );
        }
        None => {
            ui.label(format!("Branch '{branch}' will remain after removal."));
        }
    }
}

fn render_kill_all_modal(app: &mut HiveApp, ui: &mut egui::Ui) {
    let ctx = &ui.ctx().clone();
    if !app.confirm_kill_all {
        return;
    }
    let pgids = unique_pgids_in_filter(app);
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
                if ui.add(theme::danger_button("Confirm")).clicked() {
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

#[cfg(test)]
mod tests {
    //! Tiered Open-button port resolution. The four tiers — Pgid,
    //! WorktreeClaim, Declared, KnownDefault — must each be exercised, and
    //! the "exactly one unclaimed listener" guard on WorktreeClaim must hold
    //! against multi-candidate ambiguity.

    use super::*;
    use crate::runtime::ActiveRun;
    use std::time::Instant;
    use switchbard_core::types::LocalListener;

    /// The row's lavender "this holds work" signal and its `remove ok` badge
    /// must never contradict each other.
    ///
    /// They used to: the signal counted commits ahead of the *local* `main` by
    /// ancestry, while the badge asked whether the content was on
    /// `default_branch()`. A rebase-merged worktree therefore lit the drift
    /// chip, the repo card's count, the lavender dot and the auto-expand rule,
    /// all while its own badge said the work was safely upstream. On one real
    /// machine that was 9 of 41 worktrees.
    ///
    /// Both now read the same patch-equivalence probe, so this walks the two
    /// states that matter and asserts they agree in each.
    #[test]
    fn the_rows_unlanded_signal_agrees_with_its_removal_badge() {
        use switchbard_core::{Fact, LandedEvidence, TrunkDivergence, WorktreeStaleness};

        let meta = |unlanded: u32, staleness: WorktreeStaleness| crate::runtime::WorktreeMeta {
            dirty_files: Some(vec![]),
            lock: Fact::Known(None),
            trunk: Some(TrunkDivergence {
                base: "origin/main".into(),
                unlanded,
                // Ahead by two more than are at risk: the rebase-merged case
                // this test exists for.
                ancestry_ahead: unlanded + 2,
                behind: 12,
            }),
            staleness: Some(staleness),
            ..Default::default()
        };

        // Rebase-merged: ahead by ancestry, nothing at risk.
        let landed = meta(
            0,
            WorktreeStaleness::Merged {
                base: "origin/main".into(),
                evidence: LandedEvidence::PatchEquivalent,
            },
        );
        assert!(
            !has_unlanded_work(&landed.trunk),
            "a rebase-merged worktree holds nothing the trunk lacks"
        );
        assert_eq!(
            RemovalSafety::evaluate(
                &crate::runtime::removal_facts(false, &landed, AttachedProcesses::default()),
                RemovalIntent::WorktreeAndBranch,
            )
            .verdict(),
            RemovalVerdict::Safe,
            "…and the badge has to say so too"
        );

        // Genuinely unlanded: both surfaces must flag it.
        let at_risk = meta(5, WorktreeStaleness::NoUpstream);
        assert!(has_unlanded_work(&at_risk.trunk));
        assert_eq!(
            RemovalSafety::evaluate(
                &crate::runtime::removal_facts(false, &at_risk, AttachedProcesses::default()),
                RemovalIntent::WorktreeAndBranch,
            )
            .verdict(),
            RemovalVerdict::Blocked,
        );
    }

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
                source: ServiceSource::NodeScript,
                source_file: PathBuf::from("package.json"),
                likelihood: ServerLikelihood::Server,
                expected_port,
            }],
        }
    }

    #[test]
    fn ignored_files_alone_are_not_noteworthy() {
        let meta = WorktreeMeta {
            dirty_files: Some(Vec::new()),
            ignored_files: Some(crate::runtime::FileListSummary::from_lines(
                vec!["!! target/".to_string()],
                4,
            )),
            ..Default::default()
        };

        assert!(!is_noteworthy(&[], &meta, false));
    }

    fn empty_snap() -> Snapshot {
        Snapshot {
            repos: Vec::new(),
            worktrees: Vec::new(),
            meta: HashMap::new(),
            sizes: HashMap::new(),
            landing: Arc::new(Mutex::new(HashMap::new())),
            services: HashMap::new(),
            listeners_by_wt: HashMap::new(),
            unattributed: Vec::new(),
            active_runs: HashMap::new(),
            dispatch_holds_by_wt: HashMap::new(),
            by_port: HashMap::new(),
            ports_by_pgid: HashMap::new(),
            filter_lc: String::new(),
            show_only_managed: false,
            raw_detected_total: 0,
            staleness_filter: StalenessFilter::All,
        }
    }

    #[test]
    fn tier_a_pgid_match_wins() {
        let mut snap = empty_snap();
        snap.ports_by_pgid.insert(42, vec![6006]);
        let hint =
            open_port_for_running(42, &wt_path(), &resolved_service("storybook", None), &snap)
                .unwrap();
        assert_eq!(hint.port, 6006);
        assert_eq!(hint.source, OpenPortSource::Pgid);
    }

    #[test]
    fn tier_b_unclaimed_worktree_listener_when_pgid_misses() {
        // Storybook scenario: Switchbard launched the run under pgid 42, but the
        // actual worker bound :6006 under pgid 99 after detaching.
        let mut snap = empty_snap();
        snap.listeners_by_wt
            .insert(wt_path(), vec![listener(123, 99, 6006)]);
        let hint =
            open_port_for_running(42, &wt_path(), &resolved_service("storybook", None), &snap)
                .unwrap();
        assert_eq!(hint.port, 6006);
        assert_eq!(hint.source, OpenPortSource::WorktreeClaim);
    }

    #[test]
    fn tier_b_skips_listeners_claimed_by_another_active_run() {
        // A second service is already running in the same worktree and owns
        // the only listener. Don't misattribute.
        let mut snap = empty_snap();
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
        let mut snap = empty_snap();
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
        let mut snap = empty_snap();
        snap.listeners_by_wt
            .insert(other_wt_path(), vec![listener(123, 99, 6006)]);
        let hint = open_port_for_running(42, &wt_path(), &resolved_service("custom", None), &snap);
        assert!(hint.is_none());
    }

    #[test]
    fn tier_c_declared_port_fallback() {
        let snap = empty_snap();
        let hint = open_port_for_running(
            42,
            &wt_path(),
            &resolved_service("custom", Some(7777)),
            &snap,
        )
        .unwrap();
        assert_eq!(hint.port, 7777);
        assert_eq!(hint.source, OpenPortSource::Declared);
    }

    #[test]
    fn tier_d_known_default_for_canonical_name() {
        let snap = empty_snap();
        let hint =
            open_port_for_running(42, &wt_path(), &resolved_service("storybook", None), &snap)
                .unwrap();
        assert_eq!(hint.port, 6006);
        assert_eq!(hint.source, OpenPortSource::KnownDefault);
    }

    #[test]
    fn returns_none_when_no_tier_matches() {
        let snap = empty_snap();
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
        let mut snap = empty_snap();
        snap.ports_by_pgid.insert(42, vec![6007]);
        let hint = open_port_for_running(
            42,
            &wt_path(),
            &resolved_service("storybook", Some(6006)),
            &snap,
        )
        .unwrap();
        assert_eq!(hint.port, 6007);
        assert_eq!(hint.source, OpenPortSource::Pgid);
    }
}
