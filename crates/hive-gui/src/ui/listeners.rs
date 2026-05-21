//! Listeners view — grouped-by-repo/worktree central panel for attributed
//! listeners, plus the confirm-kill-all modal. Repo + branch live in the
//! section headings, not as table columns — the table itself shows only
//! port / pid / pgid / command / cwd / kill. The "Tracked repos" sidebar
//! lives in `ui::sidebar` and is rendered globally from `app::update`.

use crate::app::HiveApp;
use crate::ui::column_widths::{self, CellFont};
use crate::ui::components::{path_cell, strings, table_shell};
use crate::ui::path_display;
use crate::ui::theme;
use eframe::egui;
use egui_extras::Column;
use hive_core::{AttributedListener, WorktreeRef};
use std::collections::{BTreeSet, HashMap};
use std::path::PathBuf;

pub fn render(app: &mut HiveApp, ctx: &egui::Context) {
    let rows = snapshot_filtered(app);
    let unique_pgids = unique_pgids(&rows);
    let widths = LiColumnWidths::compute(ctx, &rows);
    render_central(app, ctx, &rows, widths);
    render_kill_all_modal(app, ctx, &unique_pgids);
}

pub fn unique_pgids_in_filter(app: &HiveApp) -> Vec<i32> {
    unique_pgids(&snapshot_filtered(app))
}

fn unique_pgids(rows: &[AttributedListener]) -> Vec<i32> {
    let mut set = BTreeSet::new();
    for r in rows {
        set.insert(r.listener.pgid);
    }
    set.into_iter().collect()
}

fn snapshot_filtered(app: &HiveApp) -> Vec<AttributedListener> {
    let filter_lc = app.filter.to_lowercase();
    let s = app.state.lock().unwrap();
    s.listeners
        .iter()
        .filter(|l| !app.show_only_managed || l.repo_name.is_some())
        .filter(|l| matches_listener_filter(l, &filter_lc))
        .cloned()
        .collect()
}

fn matches_listener_filter(l: &AttributedListener, filter_lc: &str) -> bool {
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

fn render_central(
    app: &HiveApp,
    ctx: &egui::Context,
    rows: &[AttributedListener],
    widths: LiColumnWidths,
) {
    egui::CentralPanel::default().show(ctx, |ui| {
        let mut kill_request: Option<i32> = None;
        render_grouped(app, ui, rows, widths, &mut kill_request);
        if let Some(pgid) = kill_request {
            app.spawn_kill(pgid, ctx);
        }
    });
}

fn render_kill_all_modal(app: &mut HiveApp, ctx: &egui::Context, unique_pgids: &[i32]) {
    if !app.confirm_kill_all {
        return;
    }
    let mut open = true;
    let mut do_confirm = false;
    let mut do_cancel = false;
    let pgid_count = unique_pgids.len();
    egui::Window::new("Confirm kill all")
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            ui.label(format!(
                "Send SIGTERM (then SIGKILL after 3s) to {} unique process group{} in the current filter?",
                pgid_count,
                if pgid_count == 1 { "" } else { "s" }
            ));
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui
                    .add(
                        egui::Button::new(egui::RichText::new("Confirm").color(egui::Color32::WHITE))
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
        app.spawn_kill_many(unique_pgids.to_vec(), ctx);
        app.confirm_kill_all = false;
    } else if do_cancel || !open {
        app.confirm_kill_all = false;
    }
}

fn render_grouped(
    app: &HiveApp,
    ui: &mut egui::Ui,
    rows: &[AttributedListener],
    widths: LiColumnWidths,
    kill_request: &mut Option<i32>,
) {
    type Bucket = (Option<String>, Option<PathBuf>);
    let mut by_repo_wt: HashMap<Bucket, Vec<AttributedListener>> = HashMap::new();
    for r in rows {
        by_repo_wt
            .entry((r.repo_name.clone(), r.worktree_path.clone()))
            .or_default()
            .push(r.clone());
    }

    let repos = app.repos_snapshot();
    let worktrees = app.worktrees_snapshot();

    egui::ScrollArea::vertical()
        .id_salt("listeners_grouped_scroll")
        .show(ui, |ui| {
            let mut rendered_any = false;
            for repo in &repos {
                let mut wt_groups: Vec<(&WorktreeRef, Vec<AttributedListener>)> = Vec::new();
                for w in worktrees.iter().filter(|w| w.repo_name == repo.name) {
                    if let Some(rs) =
                        by_repo_wt.get(&(Some(repo.name.clone()), Some(w.path.clone())))
                    {
                        wt_groups.push((w, rs.clone()));
                    }
                }
                let repo_only = by_repo_wt
                    .get(&(Some(repo.name.clone()), None))
                    .cloned()
                    .unwrap_or_default();

                let total: usize =
                    wt_groups.iter().map(|(_, v)| v.len()).sum::<usize>() + repo_only.len();
                if total == 0 {
                    continue;
                }
                rendered_any = true;

                ui.push_id(format!("listener_repo_section_{}", repo.name), |ui| {
                    ui.horizontal(|ui| {
                        theme::painted_dot_pulse(ui, theme::GREEN, total);
                        ui.heading(&repo.name);
                        ui.label(
                            egui::RichText::new(format!(
                                "({} listener{})",
                                total,
                                if total == 1 { "" } else { "s" }
                            ))
                            .weak(),
                        );
                    });
                    ui.add_space(2.0);

                    for (wt, rs) in &wt_groups {
                        let branch = wt.branch.as_deref().unwrap_or("(detached)");
                        ui.push_id(format!("listener_wt_{}_{}", repo.name, branch), |ui| {
                            ui.horizontal(|ui| {
                                ui.add_space(10.0);
                                ui.label(egui::RichText::new(branch).strong());
                                ui.label(
                                    egui::RichText::new(format!(
                                        "· {} listener{}",
                                        rs.len(),
                                        if rs.len() == 1 { "" } else { "s" }
                                    ))
                                    .weak(),
                                );
                                ui.label(egui::RichText::new(wt.path.display().to_string()).weak());
                            });
                            render_table(
                                ui,
                                rs,
                                widths,
                                kill_request,
                                &format!("ltable_{}_{}", repo.name, branch),
                            );
                            ui.add_space(6.0);
                        });
                    }
                    if !repo_only.is_empty() {
                        ui.push_id(format!("listener_repo_only_{}", repo.name), |ui| {
                            ui.horizontal(|ui| {
                                ui.add_space(10.0);
                                ui.label(
                                    egui::RichText::new(format!(
                                        "(this repo, no specific worktree · {} listener{})",
                                        repo_only.len(),
                                        if repo_only.len() == 1 { "" } else { "s" }
                                    ))
                                    .weak(),
                                );
                            });
                            render_table(
                                ui,
                                &repo_only,
                                widths,
                                kill_request,
                                &format!("ltable_{}_norep", repo.name),
                            );
                            ui.add_space(6.0);
                        });
                    }
                    ui.add_space(8.0);
                });
            }

            let unattributed = by_repo_wt.get(&(None, None)).cloned().unwrap_or_default();
            if !unattributed.is_empty() {
                rendered_any = true;
                ui.push_id("listener_unattributed_section", |ui| {
                    ui.horizontal(|ui| {
                        theme::painted_dot_hollow(ui, egui::Color32::GRAY);
                        ui.heading("Unattributed");
                        ui.label(
                            egui::RichText::new(format!(
                                "({} listener{})",
                                unattributed.len(),
                                if unattributed.len() == 1 { "" } else { "s" }
                            ))
                            .weak(),
                        );
                    });
                    ui.add_space(2.0);
                    render_table(
                        ui,
                        &unattributed,
                        widths,
                        kill_request,
                        "ltable_unattributed",
                    );
                });
            }

            if !rendered_any {
                ui.label(egui::RichText::new("No listeners match the current filter.").weak());
            }
        });
}

/// Single listener-table renderer. Columns: PORT / PID / PGID / COMMAND /
/// CWD / ACTION. Repo + branch are NOT columns — they live in the section
/// headings the grouped renderer emits above each table.
fn render_table(
    ui: &mut egui::Ui,
    rows: &[AttributedListener],
    widths: LiColumnWidths,
    kill_request: &mut Option<i32>,
    id_salt: &str,
) {
    table_shell(ui, id_salt)
        .column(Column::initial(widths.port).at_least(50.0))
        .column(Column::initial(widths.pid).at_least(50.0))
        .column(Column::initial(widths.pgid).at_least(50.0))
        .column(Column::initial(widths.command).at_least(80.0))
        .column(Column::initial(widths.cwd).at_least(180.0))
        .column(Column::initial(widths.action).at_least(60.0))
        .header(24.0, |mut h| {
            h.col(|ui| {
                ui.strong(strings::COL_PORT);
            });
            h.col(|ui| {
                ui.strong(strings::COL_PID);
            });
            h.col(|ui| {
                ui.strong(strings::COL_PGID);
            });
            h.col(|ui| {
                ui.strong(strings::COL_COMMAND);
            });
            h.col(|ui| {
                ui.strong(strings::COL_CWD);
            });
            h.col(|ui| {
                ui.strong(strings::COL_ACTION);
            });
        })
        .body(|mut body| {
            for row in rows {
                let l = &row.listener;
                body.row(24.0, |mut r| {
                    r.col(|ui| {
                        ui.label(egui::RichText::new(l.port.to_string()).monospace().strong());
                    });
                    r.col(|ui| {
                        ui.label(egui::RichText::new(l.pid.to_string()).monospace());
                    });
                    r.col(|ui| {
                        ui.label(egui::RichText::new(l.pgid.to_string()).monospace());
                    });
                    r.col(|ui| {
                        ui.add(egui::Label::new(&l.command_name).truncate())
                            .on_hover_text(&l.command_name);
                    });
                    r.col(|ui| match &l.cwd {
                        Some(p) => {
                            path_cell(ui, p);
                        }
                        None => {
                            ui.label(egui::RichText::new("(unknown)").weak());
                        }
                    });
                    r.col(|ui| {
                        if ui.button("Kill").clicked() {
                            *kill_request = Some(l.pgid);
                        }
                    });
                });
            }
        });
}

/// Shared widths for every column in the Listeners table, pre-measured once
/// over every visible row so the Grouped sub-tables line up. CWD is sized to
/// the widest *elided* form (`…/parent/leaf`) — no Remainder column, so the
/// table doesn't balloon to fill the panel width when paths are short.
#[derive(Debug, Clone, Copy)]
struct LiColumnWidths {
    port: f32,
    pid: f32,
    pgid: f32,
    command: f32,
    cwd: f32,
    action: f32,
}

impl LiColumnWidths {
    fn compute(ctx: &egui::Context, rows: &[AttributedListener]) -> Self {
        let port_strs: Vec<String> = rows.iter().map(|r| r.listener.port.to_string()).collect();
        let pid_strs: Vec<String> = rows.iter().map(|r| r.listener.pid.to_string()).collect();
        let pgid_strs: Vec<String> = rows.iter().map(|r| r.listener.pgid.to_string()).collect();
        let cmd_strs: Vec<String> = rows
            .iter()
            .map(|r| r.listener.command_name.clone())
            .collect();
        let cwd_strs: Vec<String> = rows
            .iter()
            .filter_map(|r| r.listener.cwd.as_deref().map(path_display::shorten))
            .collect();

        let port = column_widths::column_width(
            ctx,
            std::iter::once(strings::COL_PORT).chain(port_strs.iter().map(String::as_str)),
            CellFont::Monospace,
            60.0,
        );
        let pid = column_widths::column_width(
            ctx,
            std::iter::once(strings::COL_PID).chain(pid_strs.iter().map(String::as_str)),
            CellFont::Monospace,
            60.0,
        );
        let pgid = column_widths::column_width(
            ctx,
            std::iter::once(strings::COL_PGID).chain(pgid_strs.iter().map(String::as_str)),
            CellFont::Monospace,
            60.0,
        );
        // COMMAND can be a long argv[0] — cap and truncate with hover.
        let command = column_widths::column_width_clamped(
            ctx,
            std::iter::once(strings::COL_COMMAND).chain(cmd_strs.iter().map(String::as_str)),
            CellFont::Proportional,
            100.0,
            220.0,
        );
        let cwd = column_widths::column_width(
            ctx,
            std::iter::once(strings::COL_CWD).chain(cwd_strs.iter().map(String::as_str)),
            CellFont::Proportional,
            180.0,
        );
        // Action cell is a single "Kill" button — measured against header.
        let action_header =
            column_widths::measure(ctx, strings::COL_ACTION, CellFont::Proportional);
        let kill_btn = column_widths::measure(ctx, "Kill", CellFont::Proportional) + 16.0;
        let action = action_header.max(kill_btn).max(60.0) + column_widths::COL_PADDING;

        Self {
            port,
            pid,
            pgid,
            command,
            cwd,
            action,
        }
    }
}
