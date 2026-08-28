//! Top-level Agents view and its Context / Hooks information architecture.

use crate::app::HiveApp;
use crate::runtime::{AgentContextAgent, AgentsSection};
use crate::ui::{agent_context, agent_hooks, filter_bar, theme};
use eframe::egui;
use std::collections::BTreeSet;
use switchbard_core::{ContextKind, ContextScope};

pub fn render(app: &mut HiveApp, ui: &mut egui::Ui) {
    egui::CentralPanel::default().show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.heading("Agents");
            ui.label(
                egui::RichText::new("what agents read and run in each repo")
                    .color(theme::muted_text()),
            );
        });
        ui.add_space(4.0);
        render_section_switcher(app, ui);
        ui.add_space(6.0);
        render_filters(app, ui);
        ui.add_space(8.0);
        match app.agent_context_view.section {
            AgentsSection::Context => agent_context::render(app, ui),
            AgentsSection::Hooks => agent_hooks::render(app, ui),
        }
    });
}

fn render_filters(app: &mut HiveApp, ui: &mut egui::Ui) {
    let section = app.agent_context_view.section;
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
            }
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
