//! Detected settings hook registrations within the Agents view.
//!
//! The context scanner finds hook files as assets. This surface deliberately
//! shows only registrations parsed from settings, because a script existing on
//! disk does not mean an agent will run it.

use crate::app::HiveApp;
use crate::runtime::AgentContextAgent;
use crate::ui::theme;
use eframe::egui;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use switchbard_core::{
    AgentContextMap, AgentHook, AgentHookWarning, AgentKind, ContextScope, Repo, WorktreeRef,
};

struct Snapshot {
    repos: Vec<Repo>,
    worktrees: Vec<WorktreeRef>,
    maps: BTreeMap<PathBuf, AgentContextMap>,
    filter_lc: String,
}

pub fn render(app: &mut HiveApp, ui: &mut egui::Ui) {
    let snap = snapshot(app);
    render_summary(ui, &snap);
    ui.add_space(6.0);
    egui::ScrollArea::vertical()
        .id_salt("agent_hooks_scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| render_repos(ui, app, &snap));
}

fn snapshot(app: &HiveApp) -> Snapshot {
    Snapshot {
        repos: app.repos_snapshot(),
        worktrees: app.worktrees_snapshot(),
        maps: app
            .agent_contexts
            .lock()
            .expect("invariant: agent context cache lock")
            .clone()
            .into_iter()
            .collect(),
        filter_lc: app.filter().to_lowercase(),
    }
}

fn render_summary(ui: &mut egui::Ui, snap: &Snapshot) {
    let hook_ids: BTreeSet<&str> = snap
        .maps
        .values()
        .flat_map(|map| &map.hooks)
        .map(|hook| hook.id.as_str())
        .collect();
    let warning_sources: BTreeSet<&Path> = snap
        .maps
        .values()
        .flat_map(|map| &map.hook_warnings)
        .map(|warning| warning.source_path.as_path())
        .collect();
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("Hooks").strong());
        ui.label(
            egui::RichText::new(format!("{} configured", hook_ids.len())).color(theme::lavender()),
        );
        if !warning_sources.is_empty() {
            ui.colored_label(
                theme::amber(),
                format!("{} warnings", warning_sources.len()),
            );
        }
        ui.label(
            egui::RichText::new("detected global + repo registrations").color(theme::muted_text()),
        );
    });
}

fn render_repos(ui: &mut egui::Ui, app: &HiveApp, snap: &Snapshot) {
    for repo in &snap.repos {
        let worktrees: Vec<&WorktreeRef> = snap
            .worktrees
            .iter()
            .filter(|worktree| worktree.repo_name == repo.name)
            .collect();
        let Some(worktree) = selected_worktree(repo, &worktrees, snap) else {
            continue;
        };
        render_repo(ui, app, repo, worktree, snap);
        ui.add_space(8.0);
    }
}

fn selected_worktree<'a>(
    repo: &Repo,
    worktrees: &'a [&'a WorktreeRef],
    snap: &Snapshot,
) -> Option<&'a WorktreeRef> {
    worktrees
        .iter()
        .copied()
        .find(|worktree| worktree.path == repo.path && snap.maps.contains_key(&worktree.path))
        .or_else(|| {
            worktrees
                .iter()
                .copied()
                .find(|worktree| snap.maps.contains_key(&worktree.path))
        })
        .or_else(|| worktrees.first().copied())
}

fn render_repo(
    ui: &mut egui::Ui,
    app: &HiveApp,
    repo: &Repo,
    worktree: &WorktreeRef,
    snap: &Snapshot,
) {
    let map = snap.maps.get(&worktree.path);
    let repo_text_matches = snap.filter_lc.is_empty()
        || repo.name.to_lowercase().contains(&snap.filter_lc)
        || repo
            .path
            .to_string_lossy()
            .to_lowercase()
            .contains(&snap.filter_lc);
    let hook_filter = if repo_text_matches {
        ""
    } else {
        &snap.filter_lc
    };
    if !repo_matches(
        map,
        app.agent_context_view.agent,
        hook_filter,
        repo_text_matches,
        app.agent_context_view.hook_scope,
        app.agent_context_view.hook_event.as_deref(),
        app.agent_context_view.hook_type.as_deref(),
    ) {
        return;
    }
    egui::Frame::group(ui.style())
        .inner_margin(egui::Margin::symmetric(10, 8))
        .show(ui, |ui| {
            render_repo_header(ui, app, repo, worktree, map, hook_filter);
            ui.add_space(6.0);
            match map {
                Some(map) => render_hook_list(
                    ui,
                    app.agent_context_view.agent,
                    map,
                    hook_filter,
                    app.agent_context_view.hook_scope,
                    app.agent_context_view.hook_event.as_deref(),
                    app.agent_context_view.hook_type.as_deref(),
                ),
                None => {
                    ui.label(
                        egui::RichText::new("hook settings scanning...").color(theme::muted_text()),
                    );
                }
            }
        });
}

fn render_repo_header(
    ui: &mut egui::Ui,
    app: &HiveApp,
    repo: &Repo,
    worktree: &WorktreeRef,
    map: Option<&AgentContextMap>,
    filter_lc: &str,
) {
    let hooks = map.map_or(0, |map| {
        visible_hooks(
            map,
            app.agent_context_view.agent,
            filter_lc,
            app.agent_context_view.hook_scope,
            app.agent_context_view.hook_event.as_deref(),
            app.agent_context_view.hook_type.as_deref(),
        )
        .len()
    });
    ui.horizontal(|ui| {
        theme::painted_dot(
            ui,
            if hooks == 0 {
                theme::idle_dot()
            } else {
                theme::lavender()
            },
        );
        ui.heading(&repo.name);
        ui.label(egui::RichText::new(branch(worktree)).monospace().strong());
        ui.label(egui::RichText::new(format!("{hooks} configured")).color(theme::muted_text()));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add(
                egui::Label::new(
                    egui::RichText::new(repo.path.display().to_string())
                        .small()
                        .color(theme::muted_text()),
                )
                .truncate(),
            );
        });
    });
}

fn render_hook_list(
    ui: &mut egui::Ui,
    agent: AgentContextAgent,
    map: &AgentContextMap,
    filter_lc: &str,
    scope: Option<ContextScope>,
    event: Option<&str>,
    hook_type: Option<&str>,
) {
    render_warnings(ui, &map.hook_warnings);
    if let Some(source) = &map.hooks_disabled_by {
        render_disabled(ui, source);
        return;
    }
    let hooks = visible_hooks(map, agent, filter_lc, scope, event, hook_type);
    if hooks.is_empty() {
        render_empty(
            ui,
            agent,
            !filter_lc.is_empty() || scope.is_some() || event.is_some() || hook_type.is_some(),
        );
        return;
    }
    for hook in hooks {
        render_hook(ui, hook);
        ui.add_space(4.0);
    }
}

fn render_disabled(ui: &mut egui::Ui, source: &Path) {
    egui::Frame::NONE
        .fill(theme::amber().gamma_multiply(0.08))
        .inner_margin(egui::Margin::symmetric(10, 10))
        .show(ui, |ui| {
            ui.colored_label(theme::amber(), "Hooks are disabled for this worktree");
            ui.label(
                egui::RichText::new(source.display().to_string())
                    .monospace()
                    .small()
                    .color(theme::muted_text()),
            );
        });
}

fn render_warnings(ui: &mut egui::Ui, warnings: &[AgentHookWarning]) {
    for warning in warnings {
        egui::Frame::NONE
            .fill(theme::amber().gamma_multiply(0.08))
            .inner_margin(egui::Margin::symmetric(8, 5))
            .show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.colored_label(theme::amber(), "Hook settings warning");
                    ui.label(&warning.message);
                    ui.label(
                        egui::RichText::new(warning.source_path.display().to_string())
                            .monospace()
                            .small()
                            .color(theme::muted_text()),
                    );
                });
            });
        ui.add_space(4.0);
    }
}

fn render_empty(ui: &mut egui::Ui, agent: AgentContextAgent, filters_active: bool) {
    let message = if filters_active {
        "No hooks match the current filters.".to_string()
    } else {
        format!(
            "No configured hooks detected for {} in this worktree.",
            agent.label()
        )
    };
    egui::Frame::NONE
        .fill(ui.visuals().faint_bg_color)
        .inner_margin(egui::Margin::symmetric(10, 10))
        .show(ui, |ui| {
            ui.label(egui::RichText::new(message).color(theme::muted_text()));
            ui.label(
                egui::RichText::new(
                    "Switchbard shows registrations from settings, not every script in a hooks directory.",
                )
                .small()
                .color(theme::muted_text()),
            );
        });
}

fn render_hook(ui: &mut egui::Ui, hook: &AgentHook) {
    egui::Frame::NONE
        .fill(ui.visuals().faint_bg_color)
        .stroke(theme::surface_stroke())
        .corner_radius(5.0)
        .inner_margin(egui::Margin::symmetric(8, 6))
        .show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(egui::RichText::new(hook.purpose_summary()).strong());
                ui.label(
                    egui::RichText::new("inferred from the registration")
                        .small()
                        .color(theme::muted_text()),
                );
            });
            ui.label(egui::RichText::new(hook.trigger_summary()).color(theme::muted_text()));
            if let Some(warning) = hook.configuration_warning() {
                ui.colored_label(theme::amber(), warning);
            }
            ui.add_space(3.0);
            ui.separator();
            render_hook_heading(ui, hook);
            ui.add_space(3.0);
            let action = egui::RichText::new(&hook.action).monospace();
            ui.add(egui::Label::new(action).truncate())
                .on_hover_text(&hook.action);
            if !hook.arguments.is_empty() {
                let arguments = hook
                    .arguments
                    .iter()
                    .map(|argument| format!("{argument:?}"))
                    .collect::<Vec<_>>()
                    .join(" ");
                ui.add(
                    egui::Label::new(egui::RichText::new(format!("args {arguments}")).monospace())
                        .truncate(),
                )
                .on_hover_text(arguments);
            }
            ui.label(
                egui::RichText::new(hook.source_path.display().to_string())
                    .small()
                    .monospace()
                    .color(theme::muted_text()),
            );
        });
}

fn render_hook_heading(ui: &mut egui::Ui, hook: &AgentHook) {
    ui.horizontal_wrapped(|ui| {
        ui.label(egui::RichText::new(&hook.event).strong());
        ui.label(hook.scope.label());
        ui.label(egui::RichText::new(&hook.hook_type).color(theme::lavender()));
        if let Some(matcher) = &hook.matcher {
            ui.label("matcher");
            ui.label(egui::RichText::new(matcher).monospace());
        } else {
            ui.label(egui::RichText::new("all matches").color(theme::muted_text()));
        }
        if let Some(condition) = &hook.condition {
            ui.label("if");
            ui.label(egui::RichText::new(condition).monospace());
        }
        if hook.asynchronous {
            ui.label(egui::RichText::new("async").color(theme::lavender()));
        }
        if let Some(timeout) = hook.timeout_seconds {
            ui.label(egui::RichText::new(format!("{timeout}s timeout")).color(theme::muted_text()));
        }
    });
}

fn visible_hooks<'a>(
    map: &'a AgentContextMap,
    agent: AgentContextAgent,
    filter_lc: &str,
    scope: Option<ContextScope>,
    event: Option<&str>,
    hook_type: Option<&str>,
) -> Vec<&'a AgentHook> {
    let mut hooks: Vec<&AgentHook> = map
        .hooks
        .iter()
        .filter(|hook| agent_visible(agent, hook.agent))
        .filter(|hook| scope.is_none_or(|scope| hook.scope == scope))
        .filter(|hook| event.is_none_or(|event| hook.event == event))
        .filter(|hook| hook_type.is_none_or(|hook_type| hook.hook_type == hook_type))
        .filter(|hook| hook_matches_filter(hook, filter_lc))
        .collect();
    hooks.sort_by(|a, b| {
        a.scope
            .cmp(&b.scope)
            .then(a.event.cmp(&b.event))
            .then(a.matcher.cmp(&b.matcher))
            .then(a.action.cmp(&b.action))
    });
    hooks
}

fn hook_matches_filter(hook: &AgentHook, filter_lc: &str) -> bool {
    filter_lc.is_empty()
        || hook.event.to_lowercase().contains(filter_lc)
        || hook.action.to_lowercase().contains(filter_lc)
        || hook
            .arguments
            .iter()
            .any(|argument| argument.to_lowercase().contains(filter_lc))
        || hook.hook_type.to_lowercase().contains(filter_lc)
        || hook
            .condition
            .as_deref()
            .is_some_and(|condition| condition.to_lowercase().contains(filter_lc))
        || hook
            .matcher
            .as_deref()
            .is_some_and(|matcher| matcher.to_lowercase().contains(filter_lc))
        || hook
            .source_path
            .to_string_lossy()
            .to_lowercase()
            .contains(filter_lc)
}

fn repo_matches(
    map: Option<&AgentContextMap>,
    agent: AgentContextAgent,
    filter_lc: &str,
    repo_text_matches: bool,
    scope: Option<ContextScope>,
    event: Option<&str>,
    hook_type: Option<&str>,
) -> bool {
    let facets_active = scope.is_some() || event.is_some() || hook_type.is_some();
    let hooks_match = map.is_some_and(|map| {
        !visible_hooks(map, agent, filter_lc, scope, event, hook_type).is_empty()
    });
    if facets_active || !repo_text_matches {
        hooks_match
    } else {
        true
    }
}

fn agent_visible(selected: AgentContextAgent, hook_agent: AgentKind) -> bool {
    selected == AgentContextAgent::All
        || hook_agent == AgentKind::Shared
        || hook_agent == selected.agent_kind()
}

fn branch(worktree: &WorktreeRef) -> String {
    worktree
        .branch
        .clone()
        .unwrap_or_else(|| "(detached)".to_string())
}
