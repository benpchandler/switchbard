//! Command (TASK-98): the agent-scoped fleet console — the second axis of
//! IA V2's dispatch split (trajectory: *Information architecture V2*).
//! Dispatch's task-delivery facet lives at Tasks / Dispatches
//! (`ui::places::dispatches`); this place is agent-scoped instead: one row
//! per agent session — a headless dispatch run *or* an interactive
//! `claude`/`codex` CLI a human started by hand — with mission, live
//! activity, worktree lease, SITREP age, and universal actions. Same
//! underlying runs, two axes; see that module's doc for the split.
//!
//! Also hosts the pre-existing Context/Hooks surfaces this place inherited
//! from the old top-level Agents view (`Fleet | Context | Hooks` section
//! switcher, `AgentsSection`) — TASK-96's decision record keeps them
//! reachable rather than demoting them to a different place.
//!
//! ## Fleet rows: a union of two sources, one honest about what neither knows
//!
//! - **Dispatch-origin rows** are dispatch-labeled tasks currently `InFlight`
//!   or `Failed` (`ui::backlog::dispatch_ui::dispatch_category`) — the same
//!   `DispatchRun` cache `ui::places::dispatches` reads, so a run's mission,
//!   now-line, lease, and SITREP age can never disagree between the two
//!   views. Watch/Kill/Log/Respond all reuse the *existing* verbs
//!   (`HiveApp::spawn_kill_dispatch`, `ui::dispatch::render_kill_icon`,
//!   `HiveApp::open_dispatch_path`) — no second implementation.
//! - **Interactive-origin rows** come from `switchbard_core::agent_sessions`
//!   — a live OS-process scan this app never wrote to and holds no history
//!   for. That asymmetry is real, not a bug to paper over: an interactive
//!   row's "now" line can only ever say how long the process has been
//!   running, never what it's doing (no log, no events sidecar), and it has
//!   **no Kill action at all** — killing an arbitrary process this app never
//!   spawned is a new capability this task's decision rights explicitly
//!   reserve, not an existing verb to reuse.
//!
//! ## The support-request card is evidence-only — no fabricated NEEDS_DECISION
//!
//! There is no `NEEDS_DECISION`/SITREP store anywhere in `switchbard-core`.
//! The mock's support-request card shows synthesized prose ("GitHub App or
//! PAT..."); this implementation renders only what a `DispatchRun` can prove
//! — state, elapsed, log path — for whichever selected row [`needs_you`]
//! flags, plus a Respond action that deep-links to the task and a Log action
//! that opens the run's log. Inventing decision text this app cannot back
//! with evidence would be worse than leaving the field blank. A structured
//! support-request store is real future work, named here and in the TASK-98
//! report rather than half-built.

use crate::app::HiveApp;
use crate::runtime::{
    AgentContextAgent, AgentsSection, BacklogTaskKey, CommandFacet, CommandRowKey, Place, TasksView,
};
use crate::ui::backlog::dispatch_ui::{self, DispatchState};
use crate::ui::dispatch::format_elapsed;
use crate::ui::places::dispatches::now_doing_line;
use crate::ui::theme::{self, ActionIcon};
use crate::ui::{agent_context, agent_hooks, filter_bar};
use eframe::egui;
use std::collections::BTreeSet;
use std::path::PathBuf;
use std::time::Duration;
use switchbard_core::dispatch_inspect::{now_unix, DispatchRun};
use switchbard_core::{BacklogTask, ContextKind, ContextScope, DispatchOptions};

pub fn render(app: &mut HiveApp, ui: &mut egui::Ui) {
    egui::CentralPanel::default().show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.heading("Command");
            ui.label(
                egui::RichText::new("the agent-scoped fleet console").color(theme::muted_text()),
            );
        });
        ui.add_space(4.0);
        render_section_switcher(app, ui);
        ui.add_space(6.0);
        render_filters(app, ui);
        ui.add_space(8.0);
        match app.agent_context_view.section {
            AgentsSection::Fleet => render_fleet(app, ui),
            AgentsSection::Context => agent_context::render(app, ui),
            AgentsSection::Hooks => agent_hooks::render(app, ui),
        }
    });
}

fn render_section_switcher(app: &mut HiveApp, ui: &mut egui::Ui) {
    egui::Frame::NONE
        .fill(theme::card_bg())
        .stroke(theme::surface_stroke())
        .corner_radius(6.0)
        .inner_margin(egui::Margin::symmetric(6, 4))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.selectable_value(
                    &mut app.agent_context_view.section,
                    AgentsSection::Fleet,
                    "Fleet",
                );
                ui.selectable_value(
                    &mut app.agent_context_view.section,
                    AgentsSection::Context,
                    "Context",
                );
                ui.selectable_value(
                    &mut app.agent_context_view.section,
                    AgentsSection::Hooks,
                    "Hooks",
                );
            });
        });
}

fn render_filters(app: &mut HiveApp, ui: &mut egui::Ui) {
    let section = app.agent_context_view.section;
    if section == AgentsSection::Fleet {
        render_fleet_filters(app, ui);
        return;
    }

    let (events, hook_types) = hook_options(app);
    let active_count = usize::from(!app.filter().is_empty())
        + usize::from(
            section == AgentsSection::Context
                && app.agent_context_view.scope != ContextScope::Local,
        )
        + usize::from(section == AgentsSection::Context && app.agent_context_view.kind.is_some())
        + usize::from(
            section == AgentsSection::Hooks && app.agent_context_view.hook_scope.is_some(),
        )
        + usize::from(
            section == AgentsSection::Hooks && app.agent_context_view.hook_event.is_some(),
        )
        + usize::from(
            section == AgentsSection::Hooks && app.agent_context_view.hook_type.is_some(),
        );

    filter_bar::bar(ui, active_count, |ui| {
        let hint = match section {
            AgentsSection::Context => "repo, title, path, agent, or asset type",
            AgentsSection::Hooks => "repo, event, matcher, command, or source",
            AgentsSection::Fleet => unreachable!("handled above"),
        };
        filter_bar::search(ui, "agents_local_filter", app.filter_mut(), hint);

        filter_bar::facet_label(ui, "Agent");
        egui::ComboBox::from_id_salt("agents_filter_agent")
            .selected_text(app.agent_context_view.agent.label())
            .show_ui(ui, |ui| {
                ui.selectable_value(
                    &mut app.agent_context_view.agent,
                    AgentContextAgent::Claude,
                    "Claude",
                );
                ui.selectable_value(
                    &mut app.agent_context_view.agent,
                    AgentContextAgent::Codex,
                    "Codex",
                );
                ui.selectable_value(
                    &mut app.agent_context_view.agent,
                    AgentContextAgent::All,
                    "All agents",
                );
            });

        match section {
            AgentsSection::Context => render_context_facets(app, ui),
            AgentsSection::Hooks => render_hook_facets(app, ui, &events, &hook_types),
            AgentsSection::Fleet => unreachable!("handled above"),
        }

        if filter_bar::clear(ui, active_count > 0) {
            app.filter_mut().clear();
            match section {
                AgentsSection::Context => {
                    app.agent_context_view.scope = ContextScope::Local;
                    app.agent_context_view.kind = None;
                }
                AgentsSection::Hooks => {
                    app.agent_context_view.hook_scope = None;
                    app.agent_context_view.hook_event = None;
                    app.agent_context_view.hook_type = None;
                }
                AgentsSection::Fleet => {}
            }
        }
    });
}

fn render_fleet_filters(app: &mut HiveApp, ui: &mut egui::Ui) {
    let active_count = usize::from(!app.filter().is_empty())
        + usize::from(app.command_view.facet != CommandFacet::default());
    filter_bar::bar(ui, active_count, |ui| {
        filter_bar::search(
            ui,
            "command_fleet_filter",
            app.filter_mut(),
            "agent, mission, or lease",
        );
        if filter_bar::clear(ui, active_count > 0) {
            app.filter_mut().clear();
            app.command_view.facet = CommandFacet::default();
        }
    });
}

fn render_context_facets(app: &mut HiveApp, ui: &mut egui::Ui) {
    filter_bar::facet_label(ui, "Scope");
    egui::ComboBox::from_id_salt("agents_context_scope")
        .selected_text(app.agent_context_view.scope.label())
        .show_ui(ui, |ui| {
            ui.selectable_value(
                &mut app.agent_context_view.scope,
                ContextScope::Local,
                ContextScope::Local.label(),
            );
            ui.selectable_value(
                &mut app.agent_context_view.scope,
                ContextScope::Directory,
                ContextScope::Directory.label(),
            );
        });

    filter_bar::facet_label(ui, "Type");
    egui::ComboBox::from_id_salt("agents_context_kind")
        .selected_text(
            app.agent_context_view
                .kind
                .map_or("All", ContextKind::label),
        )
        .show_ui(ui, |ui| {
            ui.selectable_value(&mut app.agent_context_view.kind, None, "All");
            for kind in [
                ContextKind::Instruction,
                ContextKind::Command,
                ContextKind::Skill,
                ContextKind::Config,
                ContextKind::Doc,
            ] {
                ui.selectable_value(&mut app.agent_context_view.kind, Some(kind), kind.label());
            }
        });
}

fn render_hook_facets(
    app: &mut HiveApp,
    ui: &mut egui::Ui,
    events: &[String],
    hook_types: &[String],
) {
    filter_bar::facet_label(ui, "Scope");
    egui::ComboBox::from_id_salt("agents_hook_scope")
        .selected_text(
            app.agent_context_view
                .hook_scope
                .map_or("All", ContextScope::label),
        )
        .show_ui(ui, |ui| {
            ui.selectable_value(&mut app.agent_context_view.hook_scope, None, "All");
            for scope in [
                ContextScope::Global,
                ContextScope::Local,
                ContextScope::Directory,
            ] {
                ui.selectable_value(
                    &mut app.agent_context_view.hook_scope,
                    Some(scope),
                    scope.label(),
                );
            }
        });

    string_facet(
        ui,
        "Event",
        "agents_hook_event",
        &mut app.agent_context_view.hook_event,
        events,
    );
    string_facet(
        ui,
        "Handler",
        "agents_hook_type",
        &mut app.agent_context_view.hook_type,
        hook_types,
    );
}

fn string_facet(
    ui: &mut egui::Ui,
    label: &str,
    id: &str,
    selected: &mut Option<String>,
    options: &[String],
) {
    filter_bar::facet_label(ui, label);
    egui::ComboBox::from_id_salt(id)
        .selected_text(selected.as_deref().unwrap_or("All"))
        .show_ui(ui, |ui| {
            ui.selectable_value(selected, None, "All");
            for option in options {
                ui.selectable_value(selected, Some(option.clone()), option);
            }
        });
}

fn hook_options(app: &HiveApp) -> (Vec<String>, Vec<String>) {
    let maps = app
        .agent_contexts
        .lock()
        .expect("invariant: agent context cache lock");
    let mut events = BTreeSet::new();
    let mut hook_types = BTreeSet::new();
    for hook in maps.values().flat_map(|map| &map.hooks) {
        events.insert(hook.event.clone());
        hook_types.insert(hook.hook_type.clone());
    }
    (
        events.into_iter().collect(),
        hook_types.into_iter().collect(),
    )
}

// ---------------------------------------------------------------------
// Fleet section
// ---------------------------------------------------------------------

enum CommandOrigin {
    // Boxed: `BacklogTask`/`DispatchRun` are large relative to
    // `Interactive`'s single `Option<PathBuf>`, and clippy's
    // `large_enum_variant` is right that leaving them inline would size
    // every `CommandRow` — including every interactive one — to the
    // dispatch variant's footprint.
    Dispatch {
        repo_root: PathBuf,
        task: Box<BacklogTask>,
        state: DispatchState,
        run: Box<DispatchRun>,
    },
    Interactive {
        worktree_path: Option<PathBuf>,
    },
}

struct CommandRow {
    key: CommandRowKey,
    agent_label: &'static str,
    mission: String,
    now_line: String,
    lease: String,
    sitrep_age: Option<Duration>,
    needs_you: bool,
    origin: CommandOrigin,
}

impl CommandRow {
    fn facet(&self) -> (bool, bool) {
        // (is_dispatch, is_interactive)
        match self.origin {
            CommandOrigin::Dispatch { .. } => (true, false),
            CommandOrigin::Interactive { .. } => (false, true),
        }
    }
}

fn render_fleet(app: &mut HiveApp, ui: &mut egui::Ui) {
    let now = now_unix();
    let stale_after = DispatchOptions::default().stale_after;
    let rows = collect_command_rows(app, now, stale_after);

    if rows.is_empty() {
        ui.add_space(24.0);
        ui.vertical_centered(|ui| {
            ui.label(egui::RichText::new("No agents running").strong());
            ui.label(
                egui::RichText::new(
                    "Dispatch a task or start an interactive claude/codex session in a tracked worktree.",
                )
                .color(theme::muted_text()),
            );
        });
        return;
    }

    render_command_facet_bar(app, ui, &rows);
    ui.add_space(6.0);

    let facet = app.command_view.facet;
    let visible: Vec<&CommandRow> = rows
        .iter()
        .filter(|row| match facet {
            CommandFacet::All => true,
            CommandFacet::Dispatch => row.facet().0,
            CommandFacet::Interactive => row.facet().1,
            CommandFacet::NeedsYou => row.needs_you,
        })
        .collect();

    egui::ScrollArea::vertical()
        .max_height(ui.available_height() * 0.6)
        .show(ui, |ui| {
            if visible.is_empty() {
                ui.add_space(12.0);
                ui.label(
                    egui::RichText::new(format!("No {} rows", facet.label().to_lowercase()))
                        .color(theme::muted_text()),
                );
            }
            for row in &visible {
                render_command_row(app, ui, row, now);
            }
        });

    if let Some(selected) = app.command_view.selected.clone() {
        if let Some(row) = rows.iter().find(|r| r.key == selected) {
            if row.needs_you {
                ui.add_space(10.0);
                render_support_card(app, ui, row, now);
            }
        }
    }
}

fn render_command_facet_bar(app: &mut HiveApp, ui: &mut egui::Ui, rows: &[CommandRow]) {
    let counts = |facet: CommandFacet| -> usize {
        rows.iter()
            .filter(|row| match facet {
                CommandFacet::All => true,
                CommandFacet::Dispatch => row.facet().0,
                CommandFacet::Interactive => row.facet().1,
                CommandFacet::NeedsYou => row.needs_you,
            })
            .count()
    };
    ui.horizontal_wrapped(|ui| {
        for facet in CommandFacet::ALL {
            let label = format!("{} · {}", facet.label(), counts(facet));
            if ui
                .selectable_label(app.command_view.facet == facet, label)
                .clicked()
            {
                app.command_view.facet = facet;
            }
        }
    });
}

/// Union dispatch-origin and interactive-origin rows, scoped to the sidebar's
/// repo selection and filtered by Fleet's own search box (see
/// `render_fleet_filters`). Dispatch rows come from the same `dispatch_runs`
/// cache `ui::places::dispatches` reads; interactive rows from
/// `HiveApp::agent_sessions_snapshot` (`workers::spawn_agent_sessions`).
fn collect_command_rows(app: &HiveApp, now: u64, stale_after: Duration) -> Vec<CommandRow> {
    let mut rows = Vec::new();
    let filter = app.filter().to_lowercase();

    let backlog_repos = app.backlog_repos_snapshot();
    let runs = app.dispatch_runs_snapshot();
    for (root, repo) in &backlog_repos {
        if !crate::runtime::path_in_scope(root, &app.repo_scope) {
            continue;
        }
        for task in &repo.tasks {
            let state = dispatch_ui::dispatch_state(task);
            if !matches!(
                state,
                DispatchState::InFlight | DispatchState::Failed { .. }
            ) {
                continue;
            }
            let Some(run) = runs
                .get(&(root.clone(), task.id.clone()) as &BacklogTaskKey)
                .cloned()
            else {
                continue;
            };
            let orphaned = matches!(state, DispatchState::InFlight) && run.is_abandoned(now, true);
            let stalled =
                matches!(state, DispatchState::InFlight) && run.looks_stalled(now, stale_after);
            let needs_you = matches!(state, DispatchState::Failed { .. }) || orphaned || stalled;
            let now_line = now_doing_line(&state, Some(&run), task, now);
            let mission = format!("{} · {}", task.id, task.title);
            let lease = format!("wt {}", run.branch);
            let sitrep_age = run
                .log_modified_unix
                .map(|t| Duration::from_secs(now.saturating_sub(t)));
            let key = CommandRowKey::Dispatch((root.clone(), task.id.clone()));
            let row = CommandRow {
                key,
                agent_label: "claude",
                mission,
                now_line,
                lease,
                sitrep_age,
                needs_you,
                origin: CommandOrigin::Dispatch {
                    repo_root: root.clone(),
                    task: Box::new(task.clone()),
                    state,
                    run: Box::new(run),
                },
            };
            if command_row_matches(&row, &filter) {
                rows.push(row);
            }
        }
    }

    let repos = app.repos_snapshot();
    for session in app.agent_sessions_snapshot() {
        let in_scope = match session
            .repo_name
            .as_ref()
            .and_then(|name| repos.iter().find(|r| &r.name == name))
        {
            Some(repo) => crate::runtime::repo_in_scope(repo, &app.repo_scope),
            // An unattributed session (no worktree matched its cwd) is
            // always shown — narrowing repo scope can't explain away a
            // session Switchbard can't place anywhere.
            None => true,
        };
        if !in_scope {
            continue;
        }
        let now_line = match session.started_unix {
            Some(started) => format!(
                "active {}",
                format_elapsed(Duration::from_secs(now.saturating_sub(started)))
            ),
            None => "active session (start time unknown)".to_string(),
        };
        let lease = match (&session.worktree_branch, &session.worktree_path) {
            (Some(branch), _) => format!("wt {branch}"),
            (None, Some(path)) => format!("wt {}", path.display()),
            (None, None) => "wt (unattributed)".to_string(),
        };
        let row = CommandRow {
            key: CommandRowKey::Interactive(session.pid),
            agent_label: session.kind.label(),
            mission: "interactive session".to_string(),
            now_line,
            lease,
            sitrep_age: None,
            needs_you: false,
            origin: CommandOrigin::Interactive {
                worktree_path: session.worktree_path.clone(),
            },
        };
        if command_row_matches(&row, &filter) {
            rows.push(row);
        }
    }

    rows
}

fn command_row_matches(row: &CommandRow, filter_lc: &str) -> bool {
    if filter_lc.is_empty() {
        return true;
    }
    [row.agent_label, row.mission.as_str(), row.lease.as_str()]
        .iter()
        .any(|field| field.to_lowercase().contains(filter_lc))
}

fn render_command_row(app: &mut HiveApp, ui: &mut egui::Ui, row: &CommandRow, now: u64) {
    let selected = app.command_view.selected.as_ref() == Some(&row.key);
    let mut frame = egui::Frame::NONE
        .inner_margin(egui::Margin::symmetric(8, 6))
        .corner_radius(4.0);
    if selected {
        frame = frame
            .fill(theme::selected_row_tint())
            .stroke(theme::selected_row_stroke());
    }
    let outer = frame.show(ui, |ui| {
        let inner = ui.scope_builder(egui::UiBuilder::new().sense(egui::Sense::click()), |ui| {
            ui.set_min_width(ui.available_width());
            ui.style_mut().interaction.selectable_labels = false;

            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(row.agent_label).strong());
                ui.label(&row.mission);
                if row.needs_you {
                    ui.colored_label(theme::amber(), "NEEDS YOU");
                }
            });
            ui.label(egui::RichText::new(&row.now_line).color(theme::muted_text()));
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(&row.lease).color(theme::muted_text()));
                let sitrep = match row.sitrep_age {
                    Some(age) => format!("SITREP {}", format_elapsed(age)),
                    None => "SITREP -".to_string(),
                };
                ui.label(egui::RichText::new(sitrep).color(theme::muted_text()));
                // Right-aligned regardless of icon count — see
                // `ui::places::dispatches::render_row`'s own comment for why
                // (`right_to_left`, first call paints furthest right)
                // rather than a fixed pixel gap (the earlier version of
                // this row had exactly that alignment bug).
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    render_command_row_actions(app, ui, row, now);
                });
            });
            // A separate, plain-layout line — see `ui::dispatch::
            // render_kill_confirm_banner`'s doc for why this is not nested
            // inside the right-aligned action cluster above.
            if let CommandOrigin::Dispatch {
                repo_root,
                task,
                state,
                run,
            } = &row.origin
            {
                if matches!(state, DispatchState::InFlight) {
                    crate::ui::dispatch::render_kill_confirm_banner(
                        app, ui, repo_root, &task.id, run,
                    );
                }
            }
        });
        inner.response
    });
    if outer.inner.clicked() {
        app.command_view.selected = Some(row.key.clone());
    }
}

/// Renders inside a `right_to_left` layout (see `render_command_row`'s call
/// site), so every branch below calls its icons in the *reverse* of their
/// intended left-to-right reading order — see `ui::places::dispatches::
/// render_row_actions`'s own doc for the rule this follows.
fn render_command_row_actions(app: &mut HiveApp, ui: &mut egui::Ui, row: &CommandRow, now: u64) {
    match &row.origin {
        CommandOrigin::Dispatch {
            repo_root,
            task,
            state,
            run,
        } => {
            let has_log = run.log_path.is_some();
            // Reads left-to-right as "Watch, Kill, Respond, Log" (needs_you)
            // or "Watch, Kill" (healthy in flight) — Watch is always called
            // last since it always reads leftmost.
            if row.needs_you {
                if theme::action_icon_button(ui, ActionIcon::Log, "Log", has_log).clicked() {
                    if let Some(path) = &run.log_path {
                        app.open_dispatch_path(path);
                    }
                }
                if theme::action_icon_button(ui, ActionIcon::Respond, "Respond", true).clicked() {
                    respond_to_task(app, repo_root, &task.id);
                }
            }
            if matches!(state, DispatchState::InFlight) {
                crate::ui::dispatch::render_kill_icon(app, ui, repo_root, &task.id, run);
            }
            if theme::action_icon_button(ui, ActionIcon::Watch, "Watch", has_log).clicked() {
                if let Some(path) = &run.log_path {
                    app.open_dispatch_path(path);
                }
            }
        }
        CommandOrigin::Interactive { worktree_path } => {
            let has_target = worktree_path.is_some();
            if theme::action_icon_button(ui, ActionIcon::Watch, "Watch", has_target).clicked() {
                if let Some(path) = worktree_path {
                    app.open_dispatch_path(path);
                }
            }
            // No Kill: killing a process this app never spawned is a new
            // capability, out of scope — see this module's doc.
        }
    }
    let _ = now;
}

/// TASK-96's `HiveApp::navigate_to_favorite` Task branch, reused verbatim:
/// switch to Tasks / All and select the task in the detail rail.
fn respond_to_task(app: &mut HiveApp, repo_root: &std::path::Path, task_id: &str) {
    app.place = Place::Tasks;
    app.tasks_view = TasksView::All;
    app.backlog_view.selected_task = Some((repo_root.to_path_buf(), task_id.to_string()));
}

/// The support-request card (mock §2c): evidence-only, never fabricated —
/// see this module's doc for why. Renders only for a selected row this
/// module's own [`CommandRow::needs_you`] flagged, which for now is always a
/// dispatch-origin row (interactive sessions never set `needs_you`).
fn render_support_card(app: &mut HiveApp, ui: &mut egui::Ui, row: &CommandRow, now: u64) {
    let CommandOrigin::Dispatch {
        repo_root,
        task,
        state,
        run,
    } = &row.origin
    else {
        return;
    };
    egui::Frame::default()
        .fill(theme::card_bg())
        .stroke(theme::surface_stroke())
        .corner_radius(6.0)
        .inner_margin(egui::Margin::same(10))
        .show(ui, |ui| {
            let elapsed = run
                .elapsed(now)
                .map(format_elapsed)
                .unwrap_or_else(|| "unknown".to_string());
            ui.label(
                egui::RichText::new(format!(
                    "Support request · {} · {}",
                    row.agent_label, elapsed
                ))
                .strong(),
            );
            let evidence = match state {
                DispatchState::Failed { reason } => {
                    format!(
                        "failed — {}",
                        reason.as_deref().unwrap_or("no reason recorded")
                    )
                }
                DispatchState::InFlight if run.is_abandoned(now, true) => {
                    "orphaned — the agent process is gone; nothing will pick this back up"
                        .to_string()
                }
                DispatchState::InFlight => {
                    "stalled — past the advisory staleness threshold, still running".to_string()
                }
                _ => "needs attention".to_string(),
            };
            ui.label(egui::RichText::new(evidence).color(theme::muted_text()));
            if let Some(log_path) = &run.log_path {
                ui.label(
                    egui::RichText::new(format!("log: {}", log_path.display()))
                        .small()
                        .color(theme::muted_text()),
                );
            }
            ui.horizontal(|ui| {
                if theme::action_icon_button(ui, ActionIcon::Respond, "Respond", true).clicked() {
                    respond_to_task(app, repo_root, &task.id);
                }
                ui.label(
                    egui::RichText::new(format!(
                        "holds only {} · rest of fleet unaffected",
                        task.id
                    ))
                    .small()
                    .color(theme::muted_text()),
                );
            });
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use switchbard_core::dispatch_inspect::{DispatchRunLiveness, RunProgress};

    const NOW: u64 = 1_000_000;

    fn dispatch_run() -> DispatchRun {
        DispatchRun {
            task_id: "TASK-1".to_string(),
            branch: "dispatch/task-1".to_string(),
            worktree_path: std::path::PathBuf::from("/repo/.worktrees/dispatch-task-1"),
            worktree_exists: true,
            log_path: None,
            prompt_path: None,
            started_at_unix: Some(NOW - 300),
            log_bytes: 0,
            log_modified_unix: None,
            liveness: DispatchRunLiveness::NoSidecar,
            progress: RunProgress::default(),
        }
    }

    fn row_for(state: DispatchState, run: DispatchRun, needs_you: bool) -> CommandRow {
        CommandRow {
            key: CommandRowKey::Dispatch((std::path::PathBuf::from("/repo"), "TASK-1".to_string())),
            agent_label: "claude",
            mission: "TASK-1 · demo".to_string(),
            now_line: String::new(),
            lease: "wt dispatch/task-1".to_string(),
            sitrep_age: None,
            needs_you,
            origin: CommandOrigin::Dispatch {
                repo_root: std::path::PathBuf::from("/repo"),
                task: Box::new(switchbard_core::BacklogTask {
                    id: "TASK-1".to_string(),
                    title: "demo".to_string(),
                    status: "In Progress".to_string(),
                    priority: "medium".to_string(),
                    assignees: vec![],
                    labels: vec![],
                    dependencies: vec![],
                    references: vec![],
                    project: None,
                    parent: None,
                    created_date: None,
                    updated_date: None,
                    description: String::new(),
                    implementation_plan: String::new(),
                    implementation_notes: String::new(),
                    final_summary: String::new(),
                    acceptance_criteria: vec![],
                    definition_of_done: vec![],
                    source: switchbard_core::BacklogTaskSource::Active,
                    path: std::path::PathBuf::from("/repo/backlog/tasks/task-1.md"),
                }),
                state,
                run: Box::new(run),
            },
        }
    }

    /// A failed dispatch row is the plainest "needs you" case — no liveness
    /// evidence required, just the terminal label.
    #[test]
    fn a_failed_row_needs_you() {
        let row = row_for(
            DispatchState::Failed {
                reason: Some("boom".to_string()),
            },
            dispatch_run(),
            true,
        );
        assert!(row.needs_you);
        assert_eq!(row.facet(), (true, false));
    }

    /// A healthy in-flight row does not need a human.
    #[test]
    fn a_healthy_in_flight_row_does_not_need_you() {
        let mut run = dispatch_run();
        run.started_at_unix = Some(NOW - 60);
        let row = row_for(DispatchState::InFlight, run, false);
        assert!(!row.needs_you);
    }

    #[test]
    fn interactive_rows_never_need_you_and_facet_as_interactive() {
        let row = CommandRow {
            key: CommandRowKey::Interactive(123),
            agent_label: "claude",
            mission: "interactive session".to_string(),
            now_line: "active 5m".to_string(),
            lease: "wt main".to_string(),
            sitrep_age: None,
            needs_you: false,
            origin: CommandOrigin::Interactive {
                worktree_path: None,
            },
        };
        assert!(!row.needs_you);
        assert_eq!(row.facet(), (false, true));
    }

    #[test]
    fn command_row_search_matches_agent_mission_or_lease() {
        let row = row_for(DispatchState::InFlight, dispatch_run(), false);
        assert!(command_row_matches(&row, ""));
        assert!(command_row_matches(&row, "claude"));
        assert!(command_row_matches(&row, "task-1"));
        assert!(command_row_matches(&row, "dispatch/task-1"));
        assert!(!command_row_matches(&row, "nope"));
    }
}
