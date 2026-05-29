//! Agent Context view.
//!
//! This is a compact scope-first explorer: pick Global / Local / Nested on the
//! left, browse matching files in the middle, and preview the selected item in
//! the drawer below.

use crate::app::HiveApp;
use crate::runtime::AgentContextAgent;
use crate::ui::theme;
use eframe::egui;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use switchbard_core::{
    read_context_preview, AgentContextItem, AgentContextMap, AgentKind, ContextKind, ContextScope,
    Repo, WorktreeRef,
};

const PREVIEW_BYTES: usize = 8 * 1024;
const ITEM_ROW_HEIGHT: f32 = 28.0;
const EXPLORER_BODY_PADDING: f32 = 14.0;
const MIN_EXPLORER_BODY_HEIGHT: f32 = 96.0;
const MAX_EXPLORER_BODY_HEIGHT: f32 = 360.0;
const REPO_SCOPES: [ContextScope; 2] = [ContextScope::Local, ContextScope::Directory];
const KINDS: [ContextKind; 6] = [
    ContextKind::Instruction,
    ContextKind::Command,
    ContextKind::Skill,
    ContextKind::Config,
    ContextKind::Doc,
    ContextKind::Hook,
];

struct Snapshot {
    repos: Vec<Repo>,
    worktrees: Vec<WorktreeRef>,
    maps: BTreeMap<PathBuf, AgentContextMap>,
    filter_lc: String,
}

#[derive(Debug, Clone, Copy, Default)]
struct ContextEstimate {
    chars: u64,
    tokens: u64,
}

pub fn render(app: &mut HiveApp, ctx: &egui::Context) {
    let snap = Snapshot {
        repos: app.repos_snapshot(),
        worktrees: app.worktrees_snapshot(),
        maps: app
            .agent_contexts
            .lock()
            .unwrap()
            .clone()
            .into_iter()
            .collect(),
        filter_lc: app.filter.to_lowercase(),
    };

    egui::CentralPanel::default().show(ctx, |ui| {
        render_summary(ui, &snap);
        ui.add_space(6.0);
        render_global_card(ui, app, &snap);
        ui.add_space(6.0);
        let mut visible_repo = None;
        let scroll_output = egui::ScrollArea::vertical()
            .id_salt("agent_context_scroll")
            .auto_shrink([false, false])
            .show_viewport(ui, |ui, _viewport| {
                for repo in &snap.repos {
                    let wts: Vec<&WorktreeRef> = snap
                        .worktrees
                        .iter()
                        .filter(|w| w.repo_name == repo.name)
                        .collect();
                    if wts.is_empty() {
                        continue;
                    }
                    let response = render_repo(ui, app, repo, &wts, &snap);
                    if visible_repo.is_none() && response.rect.intersects(ui.clip_rect()) {
                        visible_repo = Some(format!("{}  ·  {}", repo.name, repo.path.display()));
                    }
                    ui.add_space(8.0);
                }
            });
        if let Some(repo) = visible_repo {
            app.agent_context_view.pinned_repo = Some(repo);
        }
        paint_sticky_repo_header(
            ui,
            scroll_output.inner_rect,
            app.agent_context_view.pinned_repo.as_deref(),
        );
    });
}

fn paint_sticky_repo_header(ui: &egui::Ui, scroll_rect: egui::Rect, label: Option<&str>) {
    let Some(label) = label else { return };
    if scroll_rect.height() <= 0.0 {
        return;
    }
    let rect = egui::Rect::from_min_size(scroll_rect.min, egui::vec2(scroll_rect.width(), 30.0));
    let painter = ui.painter();
    painter.rect_filled(rect, 0.0, ui.visuals().panel_fill);
    painter.line_segment(
        [rect.left_bottom(), rect.right_bottom()],
        egui::Stroke::new(1.0, ui.visuals().widgets.noninteractive.bg_stroke.color),
    );
    painter.text(
        rect.left_center() + egui::vec2(10.0, 0.0),
        egui::Align2::LEFT_CENTER,
        label,
        egui::FontId::proportional(15.0),
        ui.visuals().strong_text_color(),
    );
}

fn render_summary(ui: &mut egui::Ui, snap: &Snapshot) {
    let unique_items: BTreeSet<&str> = snap
        .maps
        .values()
        .flat_map(|m| &m.items)
        .map(|i| i.id.as_str())
        .collect();
    let warning_items: BTreeSet<&str> = snap
        .maps
        .values()
        .flat_map(|m| &m.items)
        .filter(|i| i.warning.is_some())
        .map(|i| i.id.as_str())
        .collect();
    let items = unique_items.len();
    let warnings = warning_items.len();
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("Agent Context").strong());
        ui.label(egui::RichText::new(format!("{items} assets")).color(theme::LAVENDER));
        if warnings > 0 {
            ui.colored_label(theme::AMBER, format!("{warnings} warnings"));
        }
        ui.label(egui::RichText::new("best-effort local scan").weak());
    });
}

fn render_global_card(ui: &mut egui::Ui, app: &mut HiveApp, snap: &Snapshot) {
    let items = global_items(snap);
    if items.is_empty() {
        return;
    }
    let warnings = items.iter().filter(|i| i.warning.is_some()).count();
    egui::Frame::group(ui.style())
        .inner_margin(egui::Margin::symmetric(10.0, 8.0))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                theme::painted_dot(ui, theme::LAVENDER);
                ui.label(egui::RichText::new("Global shared context").strong());
                ui.label(egui::RichText::new(format!("{} assets", items.len())).weak());
                if warnings > 0 {
                    ui.colored_label(theme::AMBER, format!("{warnings} warnings"));
                }
                ui.label(egui::RichText::new("available assets, not repo-specific").weak());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let label = if app.agent_context_view.global_open {
                        "Hide"
                    } else {
                        "Show"
                    };
                    if ui.button(label).clicked() {
                        app.agent_context_view.global_open = !app.agent_context_view.global_open;
                    }
                });
            });

            if !app.agent_context_view.global_open {
                return;
            }

            let selected_items = selected_global_items(app, &items);
            let selected = selected_global_item(app, &selected_items);
            ui.add_space(4.0);
            ui.horizontal_top(|ui| {
                ui.set_min_height(190.0);
                ui.vertical(|ui| {
                    ui.set_width(190.0);
                    render_global_nav(ui, app, &items);
                });
                ui.separator();
                ui.vertical(|ui| {
                    render_global_items(ui, app, &selected_items);
                });
            });
            ui.add_space(4.0);
            render_detail_drawer(ui, selected, "global");
        });
}

fn global_items(snap: &Snapshot) -> Vec<&AgentContextItem> {
    let mut by_id = BTreeMap::new();
    for item in snap.maps.values().flat_map(|m| &m.items) {
        if item.scope == ContextScope::Global {
            by_id.entry(item.id.as_str()).or_insert(item);
        }
    }
    let mut items: Vec<&AgentContextItem> = by_id.into_values().collect();
    items.sort_by(|a, b| a.kind.cmp(&b.kind).then(a.title.cmp(&b.title)));
    items
}

fn repo_item_count(app: &HiveApp, map: &AgentContextMap) -> usize {
    map.items
        .iter()
        .filter(|i| visible_repo_item(app, i))
        .count()
}

fn visible_context_estimate(app: &HiveApp, map: &AgentContextMap) -> ContextEstimate {
    estimate_items(map.items.iter().filter(|i| visible_repo_item(app, i)))
}

fn estimate_items<'a>(items: impl IntoIterator<Item = &'a AgentContextItem>) -> ContextEstimate {
    let chars = items.into_iter().map(|i| i.size_bytes).sum::<u64>();
    ContextEstimate {
        chars,
        tokens: chars.div_ceil(4),
    }
}

fn format_estimate(estimate: ContextEstimate) -> String {
    format!(
        "~{} chars / ~{} tokens",
        compact_count(estimate.chars),
        compact_count(estimate.tokens)
    )
}

fn compact_count(value: u64) -> String {
    if value >= 1_000_000 {
        format!("{:.1}M", value as f64 / 1_000_000.0)
    } else if value >= 1_000 {
        format!("{:.1}k", value as f64 / 1_000.0)
    } else {
        value.to_string()
    }
}

fn repo_warning_count(app: &HiveApp, map: &AgentContextMap) -> usize {
    map.items
        .iter()
        .filter(|i| visible_repo_item(app, i) && i.warning.is_some())
        .count()
}

fn visible_repo_item(app: &HiveApp, i: &AgentContextItem) -> bool {
    i.scope != ContextScope::Global && agent_visible(app.agent_context_view.agent, i.agent)
}

fn agent_visible(selected: AgentContextAgent, item_agent: AgentKind) -> bool {
    selected == AgentContextAgent::All
        || item_agent == AgentKind::Shared
        || item_agent == selected.agent_kind()
}

fn render_repo(
    ui: &mut egui::Ui,
    app: &mut HiveApp,
    repo: &Repo,
    wts: &[&WorktreeRef],
    snap: &Snapshot,
) -> egui::Response {
    egui::Frame::group(ui.style())
        .inner_margin(egui::Margin::symmetric(10.0, 8.0))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.heading(&repo.name);
                ui.label(egui::RichText::new("Agent Context").weak());
                let maps = wts.iter().filter_map(|w| snap.maps.get(&w.path));
                let total = maps.clone().map(|m| repo_item_count(app, m)).sum::<usize>();
                let estimate = maps.map(|m| visible_context_estimate(app, m)).fold(
                    ContextEstimate::default(),
                    |acc, estimate| ContextEstimate {
                        chars: acc.chars + estimate.chars,
                        tokens: acc.tokens + estimate.tokens,
                    },
                );
                ui.colored_label(theme::LAVENDER, format!("{total} assets"));
                ui.label(egui::RichText::new(format_estimate(estimate)).weak());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        egui::RichText::new(repo.path.display().to_string())
                            .weak()
                            .small(),
                    );
                });
            });
            ui.add_space(4.0);

            let selected = selected_worktree(repo, wts, snap);
            let Some(w) = selected else {
                ui.label(egui::RichText::new("agent context scanning…").weak());
                return;
            };
            let Some(map) = snap.maps.get(&w.path) else {
                ui.label(egui::RichText::new(format!("{} scanning…", branch(w))).weak());
                return;
            };
            if matches_filter(map, &snap.filter_lc) {
                render_worktree(ui, app, w, map);
            }
        })
        .response
}

fn selected_worktree<'a>(
    repo: &Repo,
    wts: &'a [&'a WorktreeRef],
    snap: &Snapshot,
) -> Option<&'a WorktreeRef> {
    wts.iter()
        .copied()
        .find(|w| w.path == repo.path && snap.maps.contains_key(&w.path))
        .or_else(|| {
            wts.iter()
                .copied()
                .find(|w| snap.maps.contains_key(&w.path))
        })
        .or_else(|| wts.first().copied())
}

fn render_worktree(ui: &mut egui::Ui, app: &mut HiveApp, w: &WorktreeRef, map: &AgentContextMap) {
    let namespace = scroll_namespace(&w.path);
    ui.horizontal(|ui| {
        theme::painted_dot(ui, headline_color(map));
        ui.label(egui::RichText::new(branch(w)).monospace().strong());
        ui.label(egui::RichText::new(format!("{} items", repo_item_count(app, map))).weak());
        ui.label(
            egui::RichText::new(format!(
                "visible context {}",
                format_estimate(visible_context_estimate(app, map))
            ))
            .weak(),
        );
        let warnings = repo_warning_count(app, map);
        if warnings > 0 {
            ui.colored_label(theme::AMBER, format!("{warnings} warnings"));
        }
    });

    render_context_target(ui, app, &w.path, &namespace);
    render_effective_stack(ui, map, app.agent_context_view.agent, &w.path);
    ui.add_space(4.0);

    let selected_items = selected_items(app, map);
    let selected = selected_item(app, &selected_items);
    let body_height = explorer_body_height(app, map, selected_items.len());

    ui.allocate_ui_with_layout(
        egui::vec2(ui.available_width(), body_height),
        egui::Layout::left_to_right(egui::Align::TOP),
        |ui| {
            ui.vertical(|ui| {
                ui.set_width(190.0);
                render_scope_nav(ui, app, map, &namespace, body_height);
            });
            ui.separator();
            ui.vertical(|ui| {
                render_items(ui, app, &selected_items, &namespace, body_height);
            });
        },
    );
    ui.add_space(4.0);
    render_detail_drawer(ui, selected, &namespace);
}

fn render_context_target(ui: &mut egui::Ui, app: &mut HiveApp, cwd: &Path, namespace: &str) {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("Context target:").strong());
        ui.label("Agent");
        egui::ComboBox::from_id_salt(format!("agent_context_agent_{namespace}"))
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
        ui.label("Working path");
        ui.label(
            egui::RichText::new(cwd.display().to_string())
                .monospace()
                .weak(),
        );
    });
}

fn render_effective_stack(
    ui: &mut egui::Ui,
    map: &AgentContextMap,
    agent: AgentContextAgent,
    cwd: &Path,
) {
    let stack = effective_instruction_items(map, agent, cwd);
    let estimate = estimate_items(stack.iter().copied());
    egui::Frame::none()
        .fill(ui.visuals().faint_bg_color)
        .inner_margin(egui::Margin::symmetric(6.0, 4.0))
        .show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    egui::RichText::new(format!(
                        "Effective instructions for {} at {} ({}):",
                        agent.label(),
                        cwd.display(),
                        format_estimate(estimate)
                    ))
                    .strong(),
                );
                if stack.is_empty() {
                    ui.label(egui::RichText::new("none detected").weak());
                }
                for (idx, item) in stack.iter().enumerate() {
                    if idx > 0 {
                        ui.label(egui::RichText::new(">").weak());
                    }
                    ui.label(short_path(&item.path));
                }
            });
        });
}

fn explorer_body_height(app: &HiveApp, map: &AgentContextMap, selected_item_count: usize) -> f32 {
    let nav_rows = repo_nav_row_count(app, map);
    let item_rows = selected_item_count.saturating_add(2);
    let rows = nav_rows.max(item_rows).max(1) as f32;
    (rows * ITEM_ROW_HEIGHT + EXPLORER_BODY_PADDING)
        .clamp(MIN_EXPLORER_BODY_HEIGHT, MAX_EXPLORER_BODY_HEIGHT)
}

fn repo_nav_row_count(app: &HiveApp, map: &AgentContextMap) -> usize {
    let child_rows = REPO_SCOPES
        .iter()
        .flat_map(|scope| KINDS.iter().map(move |kind| (*scope, *kind)))
        .filter(|(scope, kind)| count_for_visible(app, map, *scope, *kind) > 0)
        .count();
    REPO_SCOPES.len() + child_rows
}

fn effective_instruction_items<'a>(
    map: &'a AgentContextMap,
    agent: AgentContextAgent,
    cwd: &Path,
) -> Vec<&'a AgentContextItem> {
    let mut items: Vec<&AgentContextItem> = map
        .items
        .iter()
        .filter(|i| i.kind == ContextKind::Instruction && agent_visible(agent, i.agent))
        .filter(|i| match i.scope {
            ContextScope::Global | ContextScope::Local => true,
            ContextScope::Directory => i.applies_to.as_deref().is_some_and(|p| cwd.starts_with(p)),
        })
        .collect();
    items.sort_by(|a, b| {
        scope_order(a.scope)
            .cmp(&scope_order(b.scope))
            .then_with(|| {
                a.path
                    .components()
                    .count()
                    .cmp(&b.path.components().count())
            })
            .then(a.agent.cmp(&b.agent))
            .then(a.path.cmp(&b.path))
    });
    items
}

fn scope_order(scope: ContextScope) -> u8 {
    match scope {
        ContextScope::Global => 0,
        ContextScope::Local => 1,
        ContextScope::Directory => 2,
    }
}

fn render_global_nav(ui: &mut egui::Ui, app: &mut HiveApp, items: &[&AgentContextItem]) {
    egui::ScrollArea::vertical()
        .id_salt("agent_context_global_nav")
        .max_height(190.0)
        .show(ui, |ui| {
            let selected = app.agent_context_view.global_kind.is_none();
            if ui
                .selectable_label(selected, format!("Global  {}", items.len()))
                .clicked()
            {
                select_global_group(app, None);
            }
            ui.indent("global_scope", |ui| {
                for kind in KINDS {
                    let count = items.iter().filter(|i| i.kind == kind).count();
                    if count == 0 {
                        continue;
                    }
                    let selected = app.agent_context_view.global_kind == Some(kind);
                    if ui
                        .selectable_label(selected, format!("{}  {count}", kind.label()))
                        .clicked()
                    {
                        select_global_group(app, Some(kind));
                    }
                }
            });
        });
}

fn render_global_items(ui: &mut egui::Ui, app: &mut HiveApp, items: &[&AgentContextItem]) {
    ui.horizontal(|ui| {
        let title = app
            .agent_context_view
            .global_kind
            .map(|kind| format!("Global · {}", kind.label()))
            .unwrap_or_else(|| "Global · All".to_string());
        ui.label(egui::RichText::new(title).strong());
        ui.label(egui::RichText::new(format!("{} items", items.len())).weak());
        ui.label(egui::RichText::new("shown once; repo cards below omit these assets").weak());
    });
    ui.separator();
    egui::ScrollArea::vertical()
        .id_salt("agent_context_global_items")
        .max_height(160.0)
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for item in items {
                render_global_item_row(ui, app, item);
            }
        });
}

fn render_global_item_row(ui: &mut egui::Ui, app: &mut HiveApp, item: &AgentContextItem) {
    let selected = app.agent_context_view.global_selected_id.as_deref() == Some(&item.id);
    ui.horizontal(|ui| {
        if ui
            .selectable_label(selected, item.kind.label())
            .on_hover_text(item.agent.label())
            .clicked()
        {
            select_global_item(app, item);
        }
        if ui
            .selectable_label(selected, &item.title)
            .on_hover_text(item.path.display().to_string())
            .clicked()
        {
            select_global_item(app, item);
        }
        path_link(ui, item, ui.available_width() - 70.0, "global");
        if item.warning.is_some() {
            ui.colored_label(theme::AMBER, "!");
        }
    });
}

fn render_scope_nav(
    ui: &mut egui::Ui,
    app: &mut HiveApp,
    map: &AgentContextMap,
    namespace: &str,
    max_height: f32,
) {
    egui::ScrollArea::vertical()
        .id_salt(format!("agent_context_scope_nav_{namespace}"))
        .max_height(max_height)
        .show(ui, |ui| {
            for scope in REPO_SCOPES {
                let count = count_in_scope_visible(app, map, scope);
                let selected =
                    app.agent_context_view.scope == scope && app.agent_context_view.kind.is_none();
                let mut label = format!("{}  {count}", scope.label());
                if has_warning(app, map, scope, None) {
                    label.push_str(" !");
                }
                if ui.selectable_label(selected, label).clicked() {
                    select_context_group(app, scope, None);
                }
                ui.indent(format!("scope_{namespace}_{scope:?}"), |ui| {
                    for kind in KINDS {
                        let count = count_for_visible(app, map, scope, kind);
                        if count == 0 {
                            continue;
                        }
                        let selected = app.agent_context_view.scope == scope
                            && app.agent_context_view.kind == Some(kind);
                        let mut label = format!("{}  {count}", kind.label());
                        if has_warning(app, map, scope, Some(kind)) {
                            label.push_str(" !");
                        }
                        if ui.selectable_label(selected, label).clicked() {
                            select_context_group(app, scope, Some(kind));
                        }
                    }
                });
            }
        });
}

fn render_items(
    ui: &mut egui::Ui,
    app: &mut HiveApp,
    items: &[&AgentContextItem],
    namespace: &str,
    max_height: f32,
) {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(selection_title(app)).strong());
        ui.label(egui::RichText::new(format!("{} items", items.len())).weak());
    });
    ui.separator();
    egui::ScrollArea::vertical()
        .id_salt(format!("agent_context_items_{namespace}"))
        .max_height((max_height - (ITEM_ROW_HEIGHT * 1.5)).max(48.0))
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for item in items {
                render_item_row(ui, app, item, namespace);
            }
        });
}

fn render_item_row(ui: &mut egui::Ui, app: &mut HiveApp, item: &AgentContextItem, namespace: &str) {
    let selected = app.agent_context_view.selected_id.as_deref() == Some(&item.id);
    ui.horizontal(|ui| {
        if ui
            .selectable_label(selected, item.scope.label())
            .on_hover_text(item.agent.label())
            .clicked()
        {
            select_item(app, item);
        }
        if ui
            .selectable_label(selected, &item.title)
            .on_hover_text(item.path.display().to_string())
            .clicked()
        {
            select_item(app, item);
        }
        path_link(ui, item, ui.available_width() - 70.0, namespace);
        if item.warning.is_some() {
            ui.colored_label(theme::AMBER, "!");
        }
    });
}

fn path_link(ui: &mut egui::Ui, item: &AgentContextItem, max_width: f32, namespace: &str) {
    let path = item.path.display().to_string();
    egui::ScrollArea::horizontal()
        .id_salt(format!("path_{namespace}_{}", item.id))
        .max_width(max_width.max(80.0))
        .show(ui, |ui| {
            if ui
                .link(egui::RichText::new(path).monospace().small().weak())
                .clicked()
            {
                reveal(&item.path);
            }
        });
}

fn render_detail_drawer(ui: &mut egui::Ui, selected: Option<&AgentContextItem>, namespace: &str) {
    ui.separator();
    let Some(item) = selected else {
        ui.label(egui::RichText::new("Select a context file to preview.").weak());
        return;
    };

    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(&item.title).strong());
        egui::ScrollArea::horizontal()
            .id_salt(format!("detail_path_{namespace}_{}", item.id))
            .max_width(ui.available_width() - 140.0)
            .show(ui, |ui| {
                if ui
                    .link(
                        egui::RichText::new(item.path.display().to_string())
                            .monospace()
                            .small(),
                    )
                    .clicked()
                {
                    reveal(&item.path);
                }
            });
        if ui.button("Open").clicked() {
            open(&item.path);
        }
        if ui.button("Reveal").clicked() {
            reveal(&item.path);
        }
    });
    ui.horizontal_wrapped(|ui| {
        ui.label(item.scope.label());
        ui.separator();
        ui.label(item.kind.label());
        ui.separator();
        ui.label(item.agent.label());
        ui.separator();
        ui.label(format!("{} bytes", item.size_bytes));
        if let Some(warning) = &item.warning {
            ui.separator();
            ui.colored_label(theme::AMBER, warning);
        }
    });
    let preview = read_context_preview(&item.path, PREVIEW_BYTES)
        .unwrap_or_else(|e| format!("failed to read preview: {e}"));
    egui::ScrollArea::vertical()
        .id_salt(format!("preview_{namespace}_{}", item.id))
        .max_height(120.0)
        .show(ui, |ui| {
            ui.add(egui::Label::new(egui::RichText::new(preview).monospace().small()).wrap());
        });
}

fn count_in_scope_visible(app: &HiveApp, map: &AgentContextMap, scope: ContextScope) -> usize {
    map.items
        .iter()
        .filter(|i| visible_repo_item(app, i) && i.scope == scope)
        .count()
}

fn count_for_visible(
    app: &HiveApp,
    map: &AgentContextMap,
    scope: ContextScope,
    kind: ContextKind,
) -> usize {
    map.items
        .iter()
        .filter(|i| visible_repo_item(app, i) && i.scope == scope && i.kind == kind)
        .count()
}

fn selected_global_items<'a>(
    app: &HiveApp,
    items: &[&'a AgentContextItem],
) -> Vec<&'a AgentContextItem> {
    let mut selected: Vec<&AgentContextItem> = items
        .iter()
        .copied()
        .filter(|i| {
            app.agent_context_view
                .global_kind
                .is_none_or(|kind| i.kind == kind)
        })
        .collect();
    selected.sort_by(|a, b| a.kind.cmp(&b.kind).then(a.title.cmp(&b.title)));
    selected
}

fn selected_global_item<'a>(
    app: &HiveApp,
    selected_items: &[&'a AgentContextItem],
) -> Option<&'a AgentContextItem> {
    app.agent_context_view
        .global_selected_id
        .as_deref()
        .and_then(|id| selected_items.iter().copied().find(|i| i.id == id))
}

fn selected_items<'a>(app: &HiveApp, map: &'a AgentContextMap) -> Vec<&'a AgentContextItem> {
    let mut items: Vec<&AgentContextItem> = map
        .items
        .iter()
        .filter(|i| visible_repo_item(app, i))
        .filter(|i| i.scope == app.agent_context_view.scope)
        .filter(|i| {
            app.agent_context_view
                .kind
                .is_none_or(|kind| i.kind == kind)
        })
        .collect();
    items.sort_by(|a, b| a.kind.cmp(&b.kind).then(a.title.cmp(&b.title)));
    items
}

fn selected_item<'a>(
    app: &HiveApp,
    selected_items: &[&'a AgentContextItem],
) -> Option<&'a AgentContextItem> {
    app.agent_context_view
        .selected_id
        .as_deref()
        .and_then(|id| selected_items.iter().copied().find(|i| i.id == id))
}

fn select_global_group(app: &mut HiveApp, kind: Option<ContextKind>) {
    app.agent_context_view.global_kind = kind;
    app.agent_context_view.global_selected_id = None;
}

fn select_global_item(app: &mut HiveApp, item: &AgentContextItem) {
    app.agent_context_view.global_selected_id = Some(item.id.clone());
}

fn select_context_group(app: &mut HiveApp, scope: ContextScope, kind: Option<ContextKind>) {
    app.agent_context_view.scope = scope;
    app.agent_context_view.kind = kind;
    app.agent_context_view.selected_id = None;
}

fn select_item(app: &mut HiveApp, item: &AgentContextItem) {
    app.agent_context_view.selected_id = Some(item.id.clone());
}

fn selection_title(app: &HiveApp) -> String {
    match app.agent_context_view.kind {
        Some(kind) => format!(
            "{} · {}",
            app.agent_context_view.scope.label(),
            kind.label()
        ),
        None => format!("{} · All", app.agent_context_view.scope.label()),
    }
}

fn matches_filter(map: &AgentContextMap, filter_lc: &str) -> bool {
    filter_lc.is_empty()
        || map.items.iter().any(|i| {
            i.scope != ContextScope::Global
                && (i.title.to_lowercase().contains(filter_lc)
                    || i.path.to_string_lossy().to_lowercase().contains(filter_lc)
                    || i.agent.label().to_lowercase().contains(filter_lc)
                    || i.kind.label().to_lowercase().contains(filter_lc))
        })
}

fn has_warning(
    app: &HiveApp,
    map: &AgentContextMap,
    scope: ContextScope,
    kind: Option<ContextKind>,
) -> bool {
    map.items.iter().any(|i| {
        i.scope == scope
            && kind.is_none_or(|k| i.kind == k)
            && agent_visible(app.agent_context_view.agent, i.agent)
            && i.warning.is_some()
    })
}

fn headline_color(map: &AgentContextMap) -> egui::Color32 {
    if map.items.iter().any(|i| i.warning.is_some()) {
        theme::AMBER
    } else if map.items.is_empty() {
        egui::Color32::GRAY
    } else {
        theme::LAVENDER
    }
}

fn scroll_namespace(path: &Path) -> String {
    path.display()
        .to_string()
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '_' })
        .collect()
}

fn branch(w: &WorktreeRef) -> String {
    w.branch.clone().unwrap_or_else(|| "(detached)".to_string())
}

fn short_path(path: &Path) -> String {
    let parts: Vec<String> = path
        .components()
        .rev()
        .take(2)
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect();
    parts.into_iter().rev().collect::<Vec<_>>().join("/")
}

fn open(path: &Path) {
    let _ = std::process::Command::new("open").arg(path).spawn();
}

fn reveal(path: &Path) {
    let _ = std::process::Command::new("open")
        .arg("-R")
        .arg(path)
        .spawn();
}
