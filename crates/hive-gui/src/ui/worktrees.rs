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
        // Short data columns auto-fit to their longest cell. PATH stays in
        // Remainder so it claims whatever's left and wraps long paths.
        .column(Column::auto().at_least(120.0)) // branch
        .column(Column::auto().at_least(70.0)) // head
        .column(Column::auto().at_least(60.0)) // status
        .column(Column::auto().at_least(70.0)) // drift
        .column(Column::auto().at_least(80.0)) // last commit
        .column(Column::auto().at_least(90.0)) // activity
        .column(Column::auto().at_least(60.0)) // listeners
        .column(Column::remainder().at_least(180.0)) // path (wraps)
        .header(24.0, |mut h| {
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
                ui.strong("DRIFT").on_hover_text(
                    "How far this branch has diverged from its upstream remote. \
                     '+N/-M' means N commits ahead of origin and M behind. \
                     '—' = in sync (or no upstream set); '…' = probe pending.",
                );
            });
            h.col(|ui| {
                ui.strong("LAST COMMIT");
            });
            h.col(|ui| {
                ui.strong("ACTIVITY").on_hover_text(
                    "Recent commit velocity on this branch. \
                     Burst = 3+ commits in the last 30min (agent hammering away); \
                     Active = at least one in the last hour; \
                     Slow = something in the last 24h; \
                     Idle = nothing recent. Hover the cell to see the subjects.",
                );
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
                let row_h = estimate_worktree_row_height(&w.path);
                body.row(row_h, |mut r| {
                    r.col(|ui| {
                        render_branch_cell(ui, w);
                    });
                    r.col(|ui| {
                        ui.label(egui::RichText::new(short_head(&w.head)).monospace());
                    });
                    r.col(|ui| {
                        render_dirty_cell(ui, &m);
                    });
                    r.col(|ui| {
                        render_drift_cell(ui, &m);
                    });
                    r.col(|ui| {
                        render_last_commit_cell(ui, &m);
                    });
                    r.col(|ui| {
                        render_activity_cell(ui, &m);
                    });
                    r.col(|ui| {
                        if listener_n > 0 {
                            ui.colored_label(theme::GREEN, format!("{listener_n}"));
                        } else {
                            ui.label(egui::RichText::new("—").weak());
                        }
                    });
                    r.col(|ui| {
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(w.path.display().to_string()).weak(),
                            )
                            .wrap(),
                        );
                    });
                });
            }
        });
}

/// Pre-compute the row height for a worktree row, based on how many lines
/// the wrapped PATH cell will take. The PATH column is the only one that can
/// span multiple lines; everything else fits in one. Conservative width
/// estimate of 50 chars per line (works for the default window size).
fn estimate_worktree_row_height(path: &std::path::Path) -> f32 {
    const CHARS_PER_LINE: usize = 50;
    const LINE_HEIGHT: f32 = 18.0;
    const MIN_ROW_HEIGHT: f32 = 24.0;
    const MAX_LINES: usize = 3;
    let lines = path
        .to_string_lossy()
        .chars()
        .count()
        .div_ceil(CHARS_PER_LINE)
        .clamp(1, MAX_LINES);
    (lines as f32 * LINE_HEIGHT + 6.0).max(MIN_ROW_HEIGHT)
}

fn render_branch_cell(ui: &mut egui::Ui, w: &WorktreeRef) {
    let branch_text = w.branch.clone().unwrap_or_else(|| "(detached)".into());
    if w.branch.is_none() {
        ui.label(egui::RichText::new(branch_text).italics().weak());
    } else {
        ui.label(egui::RichText::new(branch_text));
    }
}

/// LAST COMMIT cell with a hover that lists the most recent commit subjects.
/// Reuses `recent_commits` so it's free if the activity probe ran.
fn render_last_commit_cell(ui: &mut egui::Ui, m: &WorktreeMeta) {
    match m.head_commit_unix {
        Some(t) => {
            let resp = ui.label(humanize_age(t));
            if let Some(commits) = m.recent_commits.as_deref() {
                if !commits.is_empty() {
                    resp.on_hover_text(build_recent_commits_tooltip(commits));
                }
            }
        }
        None => {
            ui.label(egui::RichText::new("…").weak());
        }
    }
}

/// ACTIVITY cell: colored intensity label (Burst / Active / Slow / Idle) plus
/// a "+N / 1h" velocity badge when there's been recent work. Hover shows the
/// last few subjects so you can read what direction the agent is taking.
fn render_activity_cell(ui: &mut egui::Ui, m: &WorktreeMeta) {
    let Some(act) = m.activity() else {
        ui.label(egui::RichText::new("…").weak());
        return;
    };
    let (label, color) = match act.level {
        crate::runtime::ActivityLevel::Burst => ("Burst", theme::GREEN),
        crate::runtime::ActivityLevel::Active => ("Active", theme::GREEN),
        crate::runtime::ActivityLevel::Slow => ("Slow", theme::AMBER),
        crate::runtime::ActivityLevel::Idle => ("Idle", egui::Color32::GRAY),
    };
    let velocity = activity_velocity_badge(&act);
    let cell_text = match velocity {
        Some(v) => format!("{label} · {v}"),
        None => label.to_string(),
    };
    let resp = ui.colored_label(color, cell_text);
    let tooltip = build_activity_tooltip(&act, m.recent_commits.as_deref().unwrap_or(&[]));
    resp.on_hover_text(tooltip);
}

fn activity_velocity_badge(act: &crate::runtime::Activity) -> Option<String> {
    if act.count_1h > 0 {
        Some(format!("+{} / 1h", act.count_1h))
    } else if act.count_24h > 0 {
        Some(format!("+{} / 24h", act.count_24h))
    } else {
        None
    }
}

fn build_activity_tooltip(
    act: &crate::runtime::Activity,
    commits: &[hive_core::CommitSummary],
) -> String {
    let mut s = format!(
        "{} commit{} in the last hour, {} in the last 24h",
        act.count_1h,
        if act.count_1h == 1 { "" } else { "s" },
        act.count_24h,
    );
    if let Some(t) = act.newest_unix {
        s.push_str(&format!("\nNewest: {}", humanize_age(t)));
    }
    if !commits.is_empty() {
        s.push_str("\n\nRecent commits:\n");
        for c in commits.iter().take(5) {
            s.push_str(&format!(
                "  {}  ({})  {}\n",
                c.short_sha,
                humanize_age(c.committed_unix),
                c.subject
            ));
        }
        if commits.len() > 5 {
            s.push_str(&format!("  … and {} more\n", commits.len() - 5));
        }
    }
    s
}

fn build_recent_commits_tooltip(commits: &[hive_core::CommitSummary]) -> String {
    let mut s = String::from("Recent commits:\n");
    for c in commits.iter().take(5) {
        s.push_str(&format!(
            "  {}  ({})  {}\n",
            c.short_sha,
            humanize_age(c.committed_unix),
            c.subject
        ));
    }
    if commits.len() > 5 {
        s.push_str(&format!("  … and {} more\n", commits.len() - 5));
    }
    s
}

fn render_dirty_cell(ui: &mut egui::Ui, m: &WorktreeMeta) {
    match m.is_dirty() {
        Some(true) => {
            let tooltip = build_dirty_tooltip(m.dirty_files.as_deref().unwrap_or(&[]));
            ui.colored_label(theme::AMBER, "dirty")
                .on_hover_text(tooltip);
        }
        Some(false) => {
            ui.colored_label(theme::GREEN, "clean")
                .on_hover_text("no uncommitted changes");
        }
        None => {
            ui.label(egui::RichText::new("…").weak())
                .on_hover_text("probe pending");
        }
    }
}

fn render_drift_cell(ui: &mut egui::Ui, m: &WorktreeMeta) {
    match (m.ahead, m.behind) {
        (Some(0), Some(0)) => {
            ui.label(egui::RichText::new("—").weak())
                .on_hover_text(build_in_sync_tooltip(m.fetch_unix));
        }
        (Some(a), Some(b)) => {
            // Color matches the "N drifted" chip in the section header so the
            // visual link is obvious: lavender chip → lavender cell.
            ui.label(
                egui::RichText::new(format!("+{a}/-{b}"))
                    .monospace()
                    .color(theme::LAVENDER),
            )
            .on_hover_text(build_drift_tooltip(
                a,
                b,
                m.drift_detail.as_ref(),
                m.fetch_unix,
            ));
        }
        _ => {
            ui.label(egui::RichText::new("…").weak())
                .on_hover_text("upstream not set, or probe pending");
        }
    }
}

/// Format the dirty-cell tooltip: "12 changed files" header + first ~10
/// porcelain lines verbatim. Anything past the cap reads "… and N more".
fn build_dirty_tooltip(files: &[String]) -> String {
    const SHOW: usize = 10;
    let mut s = format!(
        "{} changed file{}:\n",
        files.len(),
        if files.len() == 1 { "" } else { "s" }
    );
    for line in files.iter().take(SHOW) {
        s.push_str("  ");
        s.push_str(line);
        s.push('\n');
    }
    if files.len() > SHOW {
        s.push_str(&format!("  … and {} more\n", files.len() - SHOW));
    }
    s.push_str("\nLegend: 'M ' modified, '??' untracked, 'A ' added, ' D' deleted.");
    s
}

/// Drift tooltip: counts on the first line, fetch age on the second, then a
/// blank line and the (capped) commit lists.
fn build_drift_tooltip(
    ahead: u32,
    behind: u32,
    detail: Option<&hive_core::DriftDetail>,
    fetch_unix: Option<u64>,
) -> String {
    let mut s = format!(
        "{ahead} commit{} ahead of upstream, {behind} behind\n",
        if ahead == 1 { "" } else { "s" }
    );
    s.push_str(&fetch_line(fetch_unix));
    if let Some(d) = detail {
        if !d.ahead.is_empty() {
            s.push_str(&format!(
                "\nAhead{}:\n",
                truncation_suffix(d.ahead.len(), ahead as usize, d.ahead_truncated)
            ));
            for c in &d.ahead {
                s.push_str(&format!("  {}  {}\n", c.short_sha, c.subject));
            }
        }
        if !d.behind.is_empty() {
            s.push_str(&format!(
                "\nBehind{}:\n",
                truncation_suffix(d.behind.len(), behind as usize, d.behind_truncated)
            ));
            for c in &d.behind {
                s.push_str(&format!("  {}  {}\n", c.short_sha, c.subject));
            }
        }
    }
    s
}

fn build_in_sync_tooltip(fetch_unix: Option<u64>) -> String {
    let mut s = String::from("in sync with upstream\n");
    s.push_str(&fetch_line(fetch_unix));
    s.push_str(
        "\nNote: Hive doesn't run `git fetch` — this reflects your local view \
         of origin, not what's actually there right now.",
    );
    s
}

fn fetch_line(fetch_unix: Option<u64>) -> String {
    match fetch_unix {
        Some(t) => format!("Last `git fetch`: {}", humanize_age(t)),
        None => "Last `git fetch`: never (or no remote configured)".to_string(),
    }
}

fn truncation_suffix(shown: usize, total: usize, truncated: bool) -> String {
    // We only know `shown` definitely matches if the probe wasn't truncated;
    // otherwise the rev-list count is authoritative.
    if truncated && total > shown {
        format!(" (showing {shown} of {total})")
    } else {
        String::new()
    }
}

fn short_head(sha: &str) -> &str {
    if sha.len() >= 8 {
        &sha[..8]
    } else {
        sha
    }
}
