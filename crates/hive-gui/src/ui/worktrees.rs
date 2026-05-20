//! Worktrees view — per-repo table of worktrees with branch/HEAD/dirty/drift
//! columns. Each section is wrapped in `push_id` so the stacked tables don't
//! collide on widget IDs.

use crate::app::HiveApp;
use crate::runtime::WorktreeMeta;
use crate::ui::theme;
use eframe::egui;
use egui_extras::{Column, TableBuilder};
use hive_core::{humanize_age, WorktreeRef};
use std::collections::HashMap;
use std::path::PathBuf;

pub fn render(app: &mut HiveApp, ctx: &egui::Context) {
    let listener_counts = listener_counts_by_path(app);
    let meta_snapshot: HashMap<PathBuf, WorktreeMeta> = app.meta.lock().unwrap().clone();
    let wt_filter_lc = app.wt_filter.to_lowercase();
    let repos = app.repos_snapshot();
    let worktrees = app.worktrees_snapshot();

    egui::CentralPanel::default().show(ctx, |ui| {
        ui.label(
            egui::RichText::new(format!(
                "{} worktrees across {} repos. Click Refresh to re-enumerate and re-probe.",
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

        egui::ScrollArea::vertical()
            .id_salt("worktrees_outer_scroll")
            .show(ui, |ui| {
                let mut first_rendered = true;
                for repo in &repos {
                    let Some(wts) = by_repo.get(repo.name.as_str()) else {
                        continue;
                    };
                    let visible: Vec<&WorktreeRef> = wts
                        .iter()
                        .copied()
                        .filter(|w| matches_filter(w, &wt_filter_lc))
                        .collect();
                    if !wt_filter_lc.is_empty() && visible.is_empty() {
                        continue;
                    }

                    // Hairline + breathing room between repo sections. The first
                    // visible repo gets none — the header above already separates it
                    // from the chrome.
                    if !first_rendered {
                        ui.add_space(16.0);
                        ui.separator();
                        ui.add_space(12.0);
                    }
                    first_rendered = false;

                    // Wrap the per-repo section so every widget inside (headers, chips,
                    // and the TableBuilder cells) gets a unique parent ID. Without
                    // this the cell-level widget IDs collide across stacked tables.
                    ui.push_id(format!("repo_section_{}", repo.name), |ui| {
                        render_repo_section(ui, repo, &visible, &meta_snapshot, &listener_counts);
                    });
                }
            });
    });
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
) {
    let total_listeners: usize = visible
        .iter()
        .map(|w| listener_counts.get(&w.path).copied().unwrap_or(0))
        .sum();
    let dirty_count = visible
        .iter()
        .filter(|w| meta.get(&w.path).and_then(|m| m.dirty).unwrap_or(false))
        .count();
    let drifted_count = visible
        .iter()
        .filter(|w| {
            meta.get(&w.path)
                .map(|m| m.ahead.unwrap_or(0) + m.behind.unwrap_or(0) > 0)
                .unwrap_or(false)
        })
        .count();

    ui.horizontal(|ui| {
        ui.heading(&repo.name);
        ui.label(egui::RichText::new(format!("({} wt)", visible.len())).weak());
        ui.separator();
        if total_listeners > 0 {
            ui.colored_label(theme::GREEN, format!("{total_listeners} listening"));
        }
        if dirty_count > 0 {
            ui.colored_label(theme::AMBER, format!("{dirty_count} dirty"));
        }
        if drifted_count > 0 {
            ui.colored_label(theme::LAVENDER, format!("{drifted_count} drifted"));
        }
    });
    ui.label(egui::RichText::new(repo.path.display().to_string()).weak());
    ui.add_space(6.0);

    TableBuilder::new(ui)
        .id_salt(format!("wt_table_{}", repo.name))
        .vscroll(false) // outer ScrollArea owns scrolling; per-table scroll areas would ID-collide
        .striped(true)
        .resizable(true)
        .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
        .column(Column::initial(220.0).at_least(140.0)) // branch
        .column(Column::initial(82.0).at_least(72.0)) // head
        .column(Column::initial(80.0).at_least(60.0)) // status
        .column(Column::initial(88.0).at_least(68.0)) // ahead/behind
        .column(Column::initial(96.0).at_least(78.0)) // last commit
        .column(Column::initial(80.0).at_least(60.0)) // listeners
        .column(Column::remainder().at_least(160.0)) // path
        .header(22.0, |mut h| {
            h.col(|ui| {
                ui.strong("BRANCH");
            });
            h.col(|ui| {
                ui.strong("HEAD");
            });
            h.col(|ui| {
                ui.strong("STATUS");
            });
            h.col(|ui| {
                ui.strong("AHEAD/BEHIND");
            });
            h.col(|ui| {
                ui.strong("LAST COMMIT");
            });
            h.col(|ui| {
                ui.strong("LISTENERS");
            });
            h.col(|ui| {
                ui.strong("PATH");
            });
        })
        .body(|mut body| {
            for w in visible {
                let m = meta.get(&w.path).cloned().unwrap_or_default();
                let listener_n = listener_counts.get(&w.path).copied().unwrap_or(0);
                body.row(22.0, |mut r| {
                    r.col(|ui| {
                        render_branch_cell(ui, w);
                    });
                    r.col(|ui| {
                        ui.label(egui::RichText::new(short_head(&w.head)).monospace().small());
                    });
                    r.col(|ui| {
                        render_dirty_cell(ui, m.dirty);
                    });
                    r.col(|ui| {
                        render_drift_cell(ui, m.ahead, m.behind);
                    });
                    r.col(|ui| match m.head_commit_unix {
                        Some(t) => {
                            ui.label(egui::RichText::new(humanize_age(t)).small());
                        }
                        None => {
                            ui.label(egui::RichText::new("…").weak());
                        }
                    });
                    r.col(|ui| {
                        if listener_n > 0 {
                            ui.colored_label(theme::GREEN, format!("{listener_n}"));
                        } else {
                            ui.label(egui::RichText::new("—").weak());
                        }
                    });
                    r.col(|ui| {
                        ui.label(egui::RichText::new(w.path.display().to_string()).weak());
                    });
                });
            }
        });
}

fn render_branch_cell(ui: &mut egui::Ui, w: &WorktreeRef) {
    let branch_text = w.branch.clone().unwrap_or_else(|| "(detached)".into());
    if w.branch.is_none() {
        ui.label(egui::RichText::new(branch_text).italics().weak());
    } else {
        ui.label(egui::RichText::new(branch_text));
    }
}

fn render_dirty_cell(ui: &mut egui::Ui, dirty: Option<bool>) {
    match dirty {
        Some(true) => {
            ui.colored_label(theme::AMBER, "dirty");
        }
        Some(false) => {
            ui.colored_label(theme::GREEN, "clean");
        }
        None => {
            ui.label(egui::RichText::new("…").weak());
        }
    }
}

fn render_drift_cell(ui: &mut egui::Ui, ahead: Option<u32>, behind: Option<u32>) {
    let txt = match (ahead, behind) {
        (Some(0), Some(0)) => "—".to_string(),
        (Some(a), Some(b)) => format!("+{a}/-{b}"),
        _ => "…".to_string(),
    };
    let weak = matches!(txt.as_str(), "—" | "…");
    if weak {
        ui.label(egui::RichText::new(txt).weak().small());
    } else {
        ui.label(egui::RichText::new(txt).small().monospace());
    }
}

fn short_head(sha: &str) -> &str {
    if sha.len() >= 8 {
        &sha[..8]
    } else {
        sha
    }
}
