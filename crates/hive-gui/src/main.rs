use eframe::egui;
use egui_extras::{Column, TableBuilder};
use hive_core::{
    attribute, enumerate_worktrees, kill_pgid, scan_listeners, AttributedListener, KillOutcome,
    Repo, WorktreeRef,
};
use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::{Duration, Instant};

fn main() -> eframe::Result<()> {
    let base_repos = vec![
        Repo {
            name: "alpha".into(),
            path: PathBuf::from("/Users/me/code/alpha"),
        },
        Repo {
            name: "delta".into(),
            path: PathBuf::from("/Users/me/code/delta"),
        },
        Repo {
            name: "gamma".into(),
            path: PathBuf::from("/Users/me/code/gamma"),
        },
        Repo {
            name: "beta".into(),
            path: PathBuf::from("/Users/me/code/beta"),
        },
        Repo {
            name: "hive".into(),
            path: PathBuf::from("/Users/me/code/hive"),
        },
    ];

    let worktrees = expand_worktrees(&base_repos);
    eprintln!(
        "Hive: tracking {} repos with {} total worktrees (incl. primary)",
        base_repos.len(),
        worktrees.len()
    );

    let opts = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1180.0, 720.0])
            .with_title("Hive — Local Listeners"),
        ..Default::default()
    };
    eframe::run_native(
        "Hive",
        opts,
        Box::new(|cc| Ok(Box::new(HiveApp::new(cc, base_repos, worktrees)))),
    )
}

/// For each tracked repo, ask git for the full worktree list and emit a
/// `WorktreeRef` per existing checkout. If `git worktree list` fails (the path
/// isn't a git repo, git binary missing, etc.) we still register the primary
/// path so it isn't silently dropped from attribution.
fn expand_worktrees(repos: &[Repo]) -> Vec<WorktreeRef> {
    let mut out = Vec::new();
    for repo in repos {
        let mut added_primary = false;
        if let Ok(entries) = enumerate_worktrees(&repo.path) {
            for e in entries {
                if !e.path.exists() {
                    continue; // skip phantom checkouts that were rm'd out from under git
                }
                if e.path == repo.path {
                    added_primary = true;
                }
                out.push(WorktreeRef {
                    repo_name: repo.name.clone(),
                    path: e.path,
                    branch: e.branch,
                });
            }
        }
        if !added_primary {
            out.push(WorktreeRef {
                repo_name: repo.name.clone(),
                path: repo.path.clone(),
                branch: None,
            });
        }
    }
    out
}

/// Mutex-protected `()` paired with a Condvar so the kill action can wake the
/// scanner thread immediately instead of waiting up to 3s for its sleep to expire.
type Kick = Arc<(Mutex<()>, Condvar)>;

struct HiveApp {
    repos: Vec<Repo>,
    worktrees: Vec<WorktreeRef>,
    state: Arc<Mutex<ScanState>>,
    kick: Kick,
    show_only_managed: bool,
    filter: String,
    confirm_kill_all: bool,
    expanded_repos: BTreeSet<String>,
}

#[derive(Default)]
struct ScanState {
    listeners: Vec<AttributedListener>,
    last_scan: Option<Instant>,
    last_error: Option<String>,
    last_kill_msg: Option<String>,
}

impl HiveApp {
    fn new(cc: &eframe::CreationContext<'_>, repos: Vec<Repo>, worktrees: Vec<WorktreeRef>) -> Self {
        let state = Arc::new(Mutex::new(ScanState::default()));
        let kick: Kick = Arc::new((Mutex::new(()), Condvar::new()));
        let bg_state = state.clone();
        let bg_kick = kick.clone();
        let bg_worktrees = worktrees.clone();
        let ctx = cc.egui_ctx.clone();
        thread::spawn(move || loop {
            let result = scan_listeners();
            let now = Instant::now();
            {
                let mut s = bg_state.lock().unwrap();
                match result {
                    Ok(listeners) => {
                        s.listeners = attribute(&listeners, &bg_worktrees);
                        s.last_error = None;
                    }
                    Err(e) => {
                        s.last_error = Some(e.to_string());
                    }
                }
                s.last_scan = Some(now);
            }
            ctx.request_repaint();
            let (lock, cvar) = &*bg_kick;
            let guard = lock.lock().unwrap();
            let _ = cvar.wait_timeout(guard, Duration::from_secs(3)).unwrap();
        });
        Self {
            repos,
            worktrees,
            state,
            kick,
            show_only_managed: false,
            filter: String::new(),
            confirm_kill_all: false,
            expanded_repos: BTreeSet::new(),
        }
    }

    fn spawn_kill(&self, pgid: i32, ctx: &egui::Context) {
        let state = self.state.clone();
        let kick = self.kick.clone();
        let ctx = ctx.clone();
        thread::spawn(move || {
            let msg = match kill_pgid(pgid, Duration::from_secs(3)) {
                Ok(KillOutcome::Terminated) => format!("killed pgid {pgid} (SIGTERM)"),
                Ok(KillOutcome::Killed) => format!("killed pgid {pgid} (SIGKILL)"),
                Ok(KillOutcome::NotFound) => format!("pgid {pgid} already gone"),
                Err(e) => format!("kill {pgid} failed: {e}"),
            };
            {
                let mut s = state.lock().unwrap();
                s.last_kill_msg = Some(msg);
            }
            let (lock, cvar) = &*kick;
            let _g = lock.lock().unwrap();
            cvar.notify_all();
            ctx.request_repaint();
        });
    }

    fn spawn_kill_many(&self, pgids: Vec<i32>, ctx: &egui::Context) {
        let state = self.state.clone();
        let kick = self.kick.clone();
        let ctx = ctx.clone();
        thread::spawn(move || {
            let mut terminated = 0usize;
            let mut killed = 0usize;
            let mut not_found = 0usize;
            let mut errored = 0usize;
            for pgid in &pgids {
                match kill_pgid(*pgid, Duration::from_secs(3)) {
                    Ok(KillOutcome::Terminated) => terminated += 1,
                    Ok(KillOutcome::Killed) => killed += 1,
                    Ok(KillOutcome::NotFound) => not_found += 1,
                    Err(_) => errored += 1,
                }
            }
            {
                let mut s = state.lock().unwrap();
                s.last_kill_msg = Some(format!(
                    "kill-all: {} terminated, {} killed, {} gone, {} errored ({} pgids)",
                    terminated, killed, not_found, errored, pgids.len()
                ));
            }
            let (lock, cvar) = &*kick;
            let _g = lock.lock().unwrap();
            cvar.notify_all();
            ctx.request_repaint();
        });
    }
}

impl eframe::App for HiveApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let filter_lc = self.filter.to_lowercase();
        let (rows_snapshot, last_scan, last_error, last_kill_msg, total_count, matched_count) = {
            let s = self.state.lock().unwrap();
            let rows: Vec<AttributedListener> = s
                .listeners
                .iter()
                .filter(|l| !self.show_only_managed || l.repo_name.is_some())
                .filter(|l| {
                    if filter_lc.is_empty() {
                        return true;
                    }
                    l.listener.command_name.to_lowercase().contains(&filter_lc)
                        || l.listener.port.to_string().contains(&filter_lc)
                        || l.listener.pid.to_string().contains(&filter_lc)
                        || l.listener
                            .cwd
                            .as_ref()
                            .map(|p| p.to_string_lossy().to_lowercase().contains(&filter_lc))
                            .unwrap_or(false)
                        || l.repo_name
                            .as_ref()
                            .map(|n| n.to_lowercase().contains(&filter_lc))
                            .unwrap_or(false)
                        || l.worktree_branch
                            .as_ref()
                            .map(|n| n.to_lowercase().contains(&filter_lc))
                            .unwrap_or(false)
                })
                .cloned()
                .collect();
            (
                rows,
                s.last_scan,
                s.last_error.clone(),
                s.last_kill_msg.clone(),
                s.listeners.len(),
                s.listeners.iter().filter(|l| l.repo_name.is_some()).count(),
            )
        };

        let unique_pgids_in_filter: Vec<i32> = {
            let mut set = BTreeSet::new();
            for r in &rows_snapshot {
                set.insert(r.listener.pgid);
            }
            set.into_iter().collect()
        };

        egui::TopBottomPanel::top("top").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("Hive");
                ui.label(egui::RichText::new("local listeners").weak());
                ui.separator();
                if let Some(at) = last_scan {
                    let secs = at.elapsed().as_secs();
                    ui.label(format!("{}s since last scan", secs));
                } else {
                    ui.label("scanning…");
                }
                if let Some(err) = &last_error {
                    ui.colored_label(egui::Color32::RED, format!("error: {err}"));
                }
                ui.separator();
                ui.label(format!("{} listeners", total_count));
                ui.label(format!("({matched_count} attributed)"));
                ui.separator();
                let kill_all_label = format!("Kill all in filter ({})", unique_pgids_in_filter.len());
                let can_kill_all = !unique_pgids_in_filter.is_empty();
                if ui
                    .add_enabled(can_kill_all, egui::Button::new(kill_all_label))
                    .clicked()
                {
                    self.confirm_kill_all = true;
                }
                if let Some(msg) = &last_kill_msg {
                    ui.separator();
                    ui.label(egui::RichText::new(msg).small());
                }
            });
            ui.horizontal(|ui| {
                ui.checkbox(&mut self.show_only_managed, "only attributed to a known repo");
                ui.label("filter:");
                ui.text_edit_singleline(&mut self.filter);
            });
        });

        egui::SidePanel::right("repos").resizable(true).default_width(260.0).show(ctx, |ui| {
            ui.heading("Tracked repos");
            ui.add_space(2.0);
            ui.label(egui::RichText::new(format!("{} worktrees", self.worktrees.len())).small().weak());
            ui.add_space(6.0);

            let s = self.state.lock().unwrap();
            for repo in &self.repos {
                let repo_count = s
                    .listeners
                    .iter()
                    .filter(|l| l.repo_name.as_deref() == Some(repo.name.as_str()))
                    .count();
                let repo_worktrees: Vec<&WorktreeRef> = self
                    .worktrees
                    .iter()
                    .filter(|w| w.repo_name == repo.name)
                    .collect();
                let wt_count = repo_worktrees.len();
                let expanded = self.expanded_repos.contains(&repo.name);

                let header = ui.horizontal(|ui| {
                    let color = if repo_count > 0 {
                        egui::Color32::from_rgb(120, 230, 140)
                    } else {
                        egui::Color32::GRAY
                    };
                    ui.colored_label(color, "●");
                    let arrow = if expanded { "▾" } else { "▸" };
                    let label = format!("{arrow} {} ({wt_count} wt)", repo.name);
                    let resp = ui.add(egui::Label::new(label).sense(egui::Sense::click()));
                    if resp.clicked() {
                        if expanded {
                            self.expanded_repos.remove(&repo.name);
                        } else {
                            self.expanded_repos.insert(repo.name.clone());
                        }
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if repo_count > 0 {
                            ui.label(egui::RichText::new(format!("{repo_count}")).strong());
                        } else {
                            ui.label(egui::RichText::new("—").weak());
                        }
                    });
                    resp
                });
                let _ = header;

                if self.expanded_repos.contains(&repo.name) {
                    for w in &repo_worktrees {
                        let n = s
                            .listeners
                            .iter()
                            .filter(|l| l.worktree_path.as_ref() == Some(&w.path))
                            .count();
                        ui.horizontal(|ui| {
                            ui.add_space(18.0);
                            let dot_color = if n > 0 {
                                egui::Color32::from_rgb(120, 230, 140)
                            } else {
                                egui::Color32::DARK_GRAY
                            };
                            ui.colored_label(dot_color, "•");
                            let branch = w.branch.as_deref().unwrap_or("(detached)");
                            ui.label(egui::RichText::new(branch).small());
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if n > 0 {
                                    ui.label(egui::RichText::new(format!("{n}")).small().strong());
                                }
                            });
                        });
                        ui.horizontal(|ui| {
                            ui.add_space(28.0);
                            ui.label(
                                egui::RichText::new(w.path.display().to_string())
                                    .small()
                                    .weak(),
                            );
                        });
                    }
                    ui.add_space(4.0);
                }
            }
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            let mut kill_request: Option<i32> = None;
            TableBuilder::new(ui)
                .striped(true)
                .resizable(true)
                .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
                .column(Column::initial(70.0).at_least(50.0))
                .column(Column::initial(70.0).at_least(50.0))
                .column(Column::initial(70.0).at_least(50.0))
                .column(Column::initial(130.0).at_least(80.0))
                .column(Column::initial(130.0).at_least(80.0))
                .column(Column::initial(140.0).at_least(80.0))
                .column(Column::remainder().at_least(120.0))
                .column(Column::initial(70.0).at_least(60.0))
                .header(22.0, |mut h| {
                    h.col(|ui| { ui.strong("PORT"); });
                    h.col(|ui| { ui.strong("PID"); });
                    h.col(|ui| { ui.strong("PGID"); });
                    h.col(|ui| { ui.strong("COMMAND"); });
                    h.col(|ui| { ui.strong("REPO"); });
                    h.col(|ui| { ui.strong("BRANCH"); });
                    h.col(|ui| { ui.strong("CWD"); });
                    h.col(|ui| { ui.strong("ACTION"); });
                })
                .body(|mut body| {
                    for row in &rows_snapshot {
                        let l = &row.listener;
                        body.row(20.0, |mut r| {
                            r.col(|ui| {
                                ui.label(egui::RichText::new(l.port.to_string()).monospace().strong());
                            });
                            r.col(|ui| {
                                ui.label(egui::RichText::new(l.pid.to_string()).monospace());
                            });
                            r.col(|ui| {
                                ui.label(egui::RichText::new(l.pgid.to_string()).monospace());
                            });
                            r.col(|ui| { ui.label(&l.command_name); });
                            r.col(|ui| match &row.repo_name {
                                Some(n) => {
                                    ui.colored_label(egui::Color32::from_rgb(120, 230, 140), n);
                                }
                                None => {
                                    ui.label(egui::RichText::new("—").weak());
                                }
                            });
                            r.col(|ui| match &row.worktree_branch {
                                Some(b) => {
                                    ui.label(egui::RichText::new(b).small());
                                }
                                None => {
                                    ui.label(egui::RichText::new("—").weak());
                                }
                            });
                            r.col(|ui| {
                                let text = l
                                    .cwd
                                    .as_ref()
                                    .map(|p| p.to_string_lossy().to_string())
                                    .unwrap_or_else(|| "(unknown)".into());
                                ui.label(egui::RichText::new(text).small());
                            });
                            r.col(|ui| {
                                if ui.button("Kill").clicked() {
                                    kill_request = Some(l.pgid);
                                }
                            });
                        });
                    }
                });

            if let Some(pgid) = kill_request {
                self.spawn_kill(pgid, ctx);
            }
        });

        if self.confirm_kill_all {
            let mut open = true;
            let pgid_count = unique_pgids_in_filter.len();
            let mut do_confirm = false;
            let mut do_cancel = false;
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
                            .add(egui::Button::new(
                                egui::RichText::new("Confirm").color(egui::Color32::WHITE),
                            ).fill(egui::Color32::from_rgb(180, 60, 60)))
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
                self.spawn_kill_many(unique_pgids_in_filter.clone(), ctx);
                self.confirm_kill_all = false;
            } else if do_cancel || !open {
                self.confirm_kill_all = false;
            }
        }
    }
}
