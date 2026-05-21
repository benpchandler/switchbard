//! Worktrees view — per-repo table of worktrees, assembled from
//! `ui::components` primitives. The view itself owns column-width
//! computation (since the column set is view-specific) but every cell
//! renderer is a one-liner that delegates to a component.

use crate::app::HiveApp;
use crate::runtime::WorktreeMeta;
use crate::ui::column_widths::{self, CellFont};
use crate::ui::components::{
    self, branch_label, count_badge, mono_label, path_cell, repo_section_header,
    repo_section_separator, short_sha, status_pill, strings, table_shell, weak_dots, Chip,
    StatusKind,
};
use crate::ui::path_display;
use crate::ui::theme;
use eframe::egui;
use egui_extras::Column;
use hive_core::{humanize_age, WorktreeRef};
use std::collections::HashMap;
use std::path::PathBuf;

mod tooltips;
use tooltips::{
    activity_tooltip, dirty_tooltip, drift_tooltip, in_sync_tooltip, recent_commits_tooltip,
};

/// Render the Worktrees section into the given `ui`. The parent owns the
/// scroll area + heading; this fn just stacks per-repo tables.
pub fn render(app: &HiveApp, ui: &mut egui::Ui, ctx: &egui::Context) {
    let listener_counts = listener_counts_by_path(app);
    let meta_snapshot: HashMap<PathBuf, WorktreeMeta> = app.meta.lock().unwrap().clone();
    let filter_lc = app.filter.to_lowercase();
    let repos = app.repos_snapshot();
    let worktrees = app.worktrees_snapshot();

    // Pre-compute column widths across every visible row so all per-repo
    // tables in this section line up vertically.
    let all_visible: Vec<&WorktreeRef> = worktrees
        .iter()
        .filter(|w| matches_filter(w, &filter_lc))
        .collect();
    let widths = WtColumnWidths::compute(ctx, &all_visible, &meta_snapshot, &listener_counts);

    ui.label(
        egui::RichText::new(format!(
            "{} worktrees across {} repos",
            worktrees.len(),
            repos.len()
        ))
        .weak(),
    );
    ui.add_space(8.0);

    let mut by_repo: HashMap<&str, Vec<&WorktreeRef>> = HashMap::new();
    for w in &worktrees {
        by_repo.entry(w.repo_name.as_str()).or_default().push(w);
    }

    let mut first = true;
    for repo in &repos {
        let Some(wts) = by_repo.get(repo.name.as_str()) else {
            continue;
        };
        let visible: Vec<&WorktreeRef> = wts
            .iter()
            .copied()
            .filter(|w| matches_filter(w, &filter_lc))
            .collect();
        if !filter_lc.is_empty() && visible.is_empty() {
            continue;
        }
        first = repo_section_separator(ui, first);

        ui.push_id(format!("repo_section_{}", repo.name), |ui| {
            render_repo_section(ui, repo, &visible, &meta_snapshot, &listener_counts, widths);
        });
    }
}

fn listener_counts_by_path(app: &HiveApp) -> HashMap<PathBuf, usize> {
    let s = app.state.lock().unwrap();
    let mut counts: HashMap<PathBuf, usize> = HashMap::new();
    for l in &s.listeners {
        if let Some(p) = &l.worktree_path {
            *counts.entry(p.clone()).or_default() += 1;
        }
    }
    counts
}

fn matches_filter(w: &WorktreeRef, filter_lc: &str) -> bool {
    if filter_lc.is_empty() {
        return true;
    }
    w.repo_name.to_lowercase().contains(filter_lc)
        || w.branch
            .as_deref()
            .unwrap_or("")
            .to_lowercase()
            .contains(filter_lc)
        || w.path.to_string_lossy().to_lowercase().contains(filter_lc)
}

fn render_repo_section(
    ui: &mut egui::Ui,
    repo: &hive_core::Repo,
    visible: &[&WorktreeRef],
    meta: &HashMap<PathBuf, WorktreeMeta>,
    listener_counts: &HashMap<PathBuf, usize>,
    widths: WtColumnWidths,
) {
    let total_listeners: usize = visible
        .iter()
        .map(|w| listener_counts.get(&w.path).copied().unwrap_or(0))
        .sum();
    let dirty_count = visible
        .iter()
        .filter(|w| {
            meta.get(&w.path)
                .and_then(|m| m.is_dirty())
                .unwrap_or(false)
        })
        .count();
    let drifted_count = visible
        .iter()
        .filter(|w| {
            meta.get(&w.path)
                .map(|m| m.ahead.unwrap_or(0) + m.behind.unwrap_or(0) > 0)
                .unwrap_or(false)
        })
        .count();

    let mut chips_storage: Vec<(egui::Color32, String)> = Vec::new();
    if total_listeners > 0 {
        chips_storage.push((theme::GREEN, format!("{total_listeners} listening")));
    }
    if dirty_count > 0 {
        chips_storage.push((theme::AMBER, format!("{dirty_count} dirty")));
    }
    if drifted_count > 0 {
        chips_storage.push((theme::LAVENDER, format!("{drifted_count} drifted")));
    }
    let chips: Vec<Chip<'_>> = chips_storage
        .iter()
        .map(|(c, t)| Chip {
            color: *c,
            text: t.as_str(),
        })
        .collect();
    let subtitle = format!("({} wt)", visible.len());
    let path_line = repo.path.display().to_string();
    repo_section_header(ui, &repo.name, &subtitle, &chips, Some(&path_line));

    table_shell(ui, format!("wt_table_{}", repo.name))
        .column(Column::initial(widths.branch).at_least(100.0))
        .column(Column::initial(widths.head).at_least(70.0))
        .column(Column::initial(widths.status).at_least(60.0))
        .column(Column::initial(widths.drift).at_least(70.0))
        .column(Column::initial(widths.last_commit).at_least(90.0))
        .column(Column::initial(widths.activity).at_least(90.0))
        .column(Column::initial(widths.listeners).at_least(70.0))
        .column(Column::initial(widths.path).at_least(180.0)) // path (elided single line)
        .header(24.0, |mut h| {
            h.col(|ui| {
                ui.strong(strings::COL_BRANCH);
            });
            h.col(|ui| {
                ui.strong(strings::COL_HEAD);
            });
            h.col(|ui| {
                ui.strong(strings::COL_STATUS);
            });
            h.col(|ui| {
                ui.strong(strings::COL_DRIFT)
                    .on_hover_text(strings::HOVER_DRIFT_HEADER);
            });
            h.col(|ui| {
                ui.strong(strings::COL_LAST_COMMIT);
            });
            h.col(|ui| {
                ui.strong(strings::COL_ACTIVITY)
                    .on_hover_text(strings::HOVER_ACTIVITY_HEADER);
            });
            h.col(|ui| {
                ui.strong(strings::COL_LISTENERS);
            });
            h.col(|ui| {
                ui.strong(strings::COL_PATH);
            });
        })
        .body(|mut body| {
            for w in visible {
                let m = meta.get(&w.path).cloned().unwrap_or_default();
                let listener_n = listener_counts.get(&w.path).copied().unwrap_or(0);
                body.row(24.0, |mut r| {
                    r.col(|ui| {
                        branch_label(ui, w.branch.as_deref());
                    });
                    r.col(|ui| {
                        short_sha(ui, &w.head);
                    });
                    r.col(|ui| {
                        render_status(ui, &m);
                    });
                    r.col(|ui| {
                        render_drift(ui, &m);
                    });
                    r.col(|ui| {
                        render_last_commit(ui, &m);
                    });
                    r.col(|ui| {
                        render_activity(ui, &m);
                    });
                    r.col(|ui| {
                        count_badge(ui, listener_n, theme::GREEN);
                    });
                    r.col(|ui| {
                        path_cell(ui, &w.path);
                    });
                });
            }
        });
}

fn render_status(ui: &mut egui::Ui, m: &WorktreeMeta) {
    match m.is_dirty() {
        Some(true) => {
            let tip = dirty_tooltip(m.dirty_files.as_deref().unwrap_or(&[]));
            status_pill(ui, StatusKind::Warn, "dirty", Some(&tip));
        }
        Some(false) => {
            status_pill(
                ui,
                StatusKind::Good,
                "clean",
                Some("no uncommitted changes"),
            );
        }
        None => {
            weak_dots(ui);
        }
    }
}

fn render_drift(ui: &mut egui::Ui, m: &WorktreeMeta) {
    match (m.ahead, m.behind) {
        (Some(0), Some(0)) => {
            let tip = in_sync_tooltip(m.fetch_unix);
            ui.label(egui::RichText::new("—").weak()).on_hover_text(tip);
        }
        (Some(a), Some(b)) => {
            let resp = mono_label(ui, &format!("+{a}/-{b}"), Some(theme::LAVENDER));
            resp.on_hover_text(drift_tooltip(a, b, m.drift_detail.as_ref(), m.fetch_unix));
        }
        _ => {
            ui.label(egui::RichText::new("…").weak())
                .on_hover_text("upstream not set, or probe pending");
        }
    }
}

fn render_last_commit(ui: &mut egui::Ui, m: &WorktreeMeta) {
    match m.head_commit_unix {
        Some(t) => {
            let resp = ui.label(humanize_age(t));
            if let Some(commits) = m.recent_commits.as_deref() {
                if !commits.is_empty() {
                    resp.on_hover_text(recent_commits_tooltip(commits));
                }
            }
        }
        None => weak_dots(ui),
    }
}

fn render_activity(ui: &mut egui::Ui, m: &WorktreeMeta) {
    use crate::runtime::ActivityLevel;
    let Some(act) = m.activity() else {
        weak_dots(ui);
        return;
    };
    let kind = match act.level {
        ActivityLevel::Burst | ActivityLevel::Active => StatusKind::Good,
        ActivityLevel::Slow => StatusKind::Warn,
        ActivityLevel::Idle => StatusKind::Neutral,
    };
    let label = match act.level {
        ActivityLevel::Burst => "Burst",
        ActivityLevel::Active => "Active",
        ActivityLevel::Slow => "Slow",
        ActivityLevel::Idle => "Idle",
    };
    let cell_text = match velocity_badge(&act) {
        Some(v) => format!("{label} · {v}"),
        None => label.to_string(),
    };
    let tip = activity_tooltip(&act, m.recent_commits.as_deref().unwrap_or(&[]));
    status_pill(ui, kind, cell_text, Some(&tip));
}

fn velocity_badge(act: &crate::runtime::Activity) -> Option<String> {
    if act.count_1h > 0 {
        Some(format!("+{} / 1h", act.count_1h))
    } else if act.count_24h > 0 {
        Some(format!("+{} / 24h", act.count_24h))
    } else {
        None
    }
}

/// Shared widths for every column in the Worktrees table, computed once per
/// render against all visible worktrees. Lives in this view since the column
/// set is view-specific; the *concept* of sharing widths is general and lives
/// in `ui::column_widths`.
#[derive(Debug, Clone, Copy)]
struct WtColumnWidths {
    branch: f32,
    head: f32,
    status: f32,
    drift: f32,
    last_commit: f32,
    activity: f32,
    listeners: f32,
    path: f32,
}

impl WtColumnWidths {
    fn compute(
        ctx: &egui::Context,
        rows: &[&WorktreeRef],
        meta: &HashMap<PathBuf, WorktreeMeta>,
        listener_counts: &HashMap<PathBuf, usize>,
    ) -> Self {
        use components::strings as s;
        let branch_strs: Vec<String> = rows
            .iter()
            .map(|w| w.branch.clone().unwrap_or_else(|| "(detached)".into()))
            .collect();
        // Cap the BRANCH column — long branches like
        // `dedupe/01-frontend-cross-file` would otherwise blow out the
        // column and push PATH off the right edge of the panel. The
        // `branch_label` primitive renders with `.truncate()` so anything
        // wider than the cap gets ellipsis + full name in the hover.
        let branch = column_widths::column_width_clamped(
            ctx,
            std::iter::once(s::COL_BRANCH).chain(branch_strs.iter().map(String::as_str)),
            CellFont::Proportional,
            100.0,
            240.0,
        );
        let head = column_widths::column_width(
            ctx,
            std::iter::once(s::COL_HEAD).chain(["c46ee9df"]),
            CellFont::Monospace,
            70.0,
        );
        let status = column_widths::column_width(
            ctx,
            std::iter::once(s::COL_STATUS).chain(["dirty"]),
            CellFont::Proportional,
            60.0,
        );
        let drift_strs: Vec<String> = rows
            .iter()
            .map(|w| match meta.get(&w.path) {
                Some(m) => match (m.ahead, m.behind) {
                    (Some(a), Some(b)) if a + b > 0 => format!("+{a}/-{b}"),
                    _ => "—".into(),
                },
                None => "—".into(),
            })
            .collect();
        let drift = column_widths::column_width(
            ctx,
            std::iter::once(s::COL_DRIFT).chain(drift_strs.iter().map(String::as_str)),
            CellFont::Monospace,
            70.0,
        );
        let age_strs: Vec<String> = rows
            .iter()
            .map(|w| {
                meta.get(&w.path)
                    .and_then(|m| m.head_commit_unix)
                    .map(humanize_age)
                    .unwrap_or_else(|| "…".into())
            })
            .collect();
        let last_commit = column_widths::column_width(
            ctx,
            std::iter::once(s::COL_LAST_COMMIT).chain(age_strs.iter().map(String::as_str)),
            CellFont::Proportional,
            90.0,
        );
        let activity_strs: Vec<String> = rows
            .iter()
            .map(|w| {
                meta.get(&w.path)
                    .and_then(|m| m.activity())
                    .map(activity_display_text)
                    .unwrap_or_else(|| "…".into())
            })
            .collect();
        let activity = column_widths::column_width(
            ctx,
            std::iter::once(s::COL_ACTIVITY).chain(activity_strs.iter().map(String::as_str)),
            CellFont::Proportional,
            90.0,
        );
        let listener_strs: Vec<String> = rows
            .iter()
            .map(|w| {
                let n = listener_counts.get(&w.path).copied().unwrap_or(0);
                if n > 0 {
                    n.to_string()
                } else {
                    "—".into()
                }
            })
            .collect();
        let listeners = column_widths::column_width(
            ctx,
            std::iter::once(s::COL_LISTENERS).chain(listener_strs.iter().map(String::as_str)),
            CellFont::Proportional,
            70.0,
        );
        let path_strs: Vec<String> = rows
            .iter()
            .map(|w| path_display::shorten(&w.path))
            .collect();
        let path = column_widths::column_width(
            ctx,
            std::iter::once(s::COL_PATH).chain(path_strs.iter().map(String::as_str)),
            CellFont::Proportional,
            180.0,
        );
        Self {
            branch,
            head,
            status,
            drift,
            last_commit,
            activity,
            listeners,
            path,
        }
    }
}

fn activity_display_text(act: crate::runtime::Activity) -> String {
    use crate::runtime::ActivityLevel;
    let label = match act.level {
        ActivityLevel::Burst => "Burst",
        ActivityLevel::Active => "Active",
        ActivityLevel::Slow => "Slow",
        ActivityLevel::Idle => "Idle",
    };
    let badge = if act.count_1h > 0 {
        format!(" · +{} / 1h", act.count_1h)
    } else if act.count_24h > 0 {
        format!(" · +{} / 24h", act.count_24h)
    } else {
        String::new()
    };
    format!("{label}{badge}")
}
