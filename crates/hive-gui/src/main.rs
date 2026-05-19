use eframe::egui;
use egui_extras::{Column, TableBuilder};
use hive_core::{
    attribute, detect_services, enumerate_worktrees, humanize_age, kill_pgid, open_url,
    probe_ahead_behind, probe_dirty, probe_head_commit_time, scan_listeners, spawn_in_session,
    url_for_port, AttributedListener, DetectedService, KillOutcome, LocalListener, Repo,
    WorktreeRef, BROWSER_APP_NAMES,
};
use std::collections::{BTreeSet, HashMap};
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
            .with_inner_size([1280.0, 760.0])
            .with_title("Hive — Local Listeners"),
        ..Default::default()
    };
    eframe::run_native(
        "Hive",
        opts,
        Box::new(|cc| Ok(Box::new(HiveApp::new(cc, base_repos, worktrees)))),
    )
}

fn expand_worktrees(repos: &[Repo]) -> Vec<WorktreeRef> {
    let mut out = Vec::new();
    for repo in repos {
        let mut added_primary = false;
        if let Ok(entries) = enumerate_worktrees(&repo.path) {
            for e in entries {
                if !e.path.exists() {
                    continue;
                }
                if e.path == repo.path {
                    added_primary = true;
                }
                out.push(WorktreeRef {
                    repo_name: repo.name.clone(),
                    path: e.path,
                    branch: e.branch,
                    head: e.head,
                });
            }
        }
        if !added_primary {
            out.push(WorktreeRef {
                repo_name: repo.name.clone(),
                path: repo.path.clone(),
                branch: None,
                head: String::new(),
            });
        }
    }
    out
}

/// Mutex+Condvar pair for waking sleeping threads (scanner and probe).
type Kick = Arc<(Mutex<()>, Condvar)>;

#[derive(Debug, Clone, Default)]
struct WorktreeMeta {
    dirty: Option<bool>,
    ahead: Option<u32>,
    behind: Option<u32>,
    head_commit_unix: Option<u64>,
    probed_at: Option<Instant>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ViewMode {
    Listeners,
    Worktrees,
    Servers,
}

#[derive(Debug, Clone)]
struct ActiveRun {
    worktree_path: PathBuf,
    service_name: String,
    // Surfaced via tooltip / future expanded-row detail; keep for UI v0.4.
    #[allow(dead_code)]
    command: String,
    pid: u32,
    pgid: i32,
    started_at: Instant,
    // Used by a forthcoming "Open log" action.
    #[allow(dead_code)]
    log_path: PathBuf,
}

struct HiveApp {
    repos: Vec<Repo>,
    worktrees: Vec<WorktreeRef>,
    meta: Arc<Mutex<HashMap<PathBuf, WorktreeMeta>>>,
    services: Arc<Mutex<HashMap<PathBuf, Vec<DetectedService>>>>,
    active_runs: Arc<Mutex<HashMap<i32, ActiveRun>>>,
    state: Arc<Mutex<ScanState>>,
    scanner_kick: Kick,
    probe_kick: Kick,
    view: ViewMode,
    show_only_managed: bool,
    filter: String,
    confirm_kill_all: bool,
    expanded_repos: BTreeSet<String>,
    wt_filter: String,
    server_filter: String,
    /// 0 = system default; 1..=BROWSER_APP_NAMES.len() = specific browser.
    browser_choice: usize,
    /// Last UI feedback message for the Servers view (spawn errors, etc.).
    server_msg: Arc<Mutex<Option<String>>>,
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
        let scanner_kick: Kick = Arc::new((Mutex::new(()), Condvar::new()));
        let probe_kick: Kick = Arc::new((Mutex::new(()), Condvar::new()));
        let meta: Arc<Mutex<HashMap<PathBuf, WorktreeMeta>>> = Arc::new(Mutex::new(HashMap::new()));
        let services: Arc<Mutex<HashMap<PathBuf, Vec<DetectedService>>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let active_runs: Arc<Mutex<HashMap<i32, ActiveRun>>> = Arc::new(Mutex::new(HashMap::new()));
        let server_msg: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));

        // Scanner thread.
        {
            let state = state.clone();
            let kick = scanner_kick.clone();
            let worktrees = worktrees.clone();
            let ctx = cc.egui_ctx.clone();
            thread::spawn(move || loop {
                let result = scan_listeners();
                let now = Instant::now();
                {
                    let mut s = state.lock().unwrap();
                    match result {
                        Ok(listeners) => {
                            s.listeners = attribute(&listeners, &worktrees);
                            s.last_error = None;
                        }
                        Err(e) => s.last_error = Some(e.to_string()),
                    }
                    s.last_scan = Some(now);
                }
                ctx.request_repaint();
                let (lock, cvar) = &*kick;
                let guard = lock.lock().unwrap();
                let _ = cvar.wait_timeout(guard, Duration::from_secs(3)).unwrap();
            });
        }

        // Git-probe thread. Walks each worktree, runs dirty/ahead/behind/last-commit
        // probes, stores into the shared map, requests a repaint after each one so
        // the table fills in incrementally instead of waiting for the whole sweep.
        {
            let meta = meta.clone();
            let kick = probe_kick.clone();
            let worktrees = worktrees.clone();
            let ctx = cc.egui_ctx.clone();
            thread::spawn(move || loop {
                for w in &worktrees {
                    let mut m = WorktreeMeta::default();
                    m.dirty = probe_dirty(&w.path);
                    m.head_commit_unix = probe_head_commit_time(&w.path);
                    if let Some((a, b)) = probe_ahead_behind(&w.path) {
                        m.ahead = Some(a);
                        m.behind = Some(b);
                    }
                    m.probed_at = Some(Instant::now());
                    {
                        let mut map = meta.lock().unwrap();
                        map.insert(w.path.clone(), m);
                    }
                    ctx.request_repaint();
                }
                // Refresh every 60s, but wake on demand for an immediate re-probe.
                let (lock, cvar) = &*kick;
                let guard = lock.lock().unwrap();
                let _ = cvar.wait_timeout(guard, Duration::from_secs(60)).unwrap();
            });
        }

        // Service-detection thread: walks every worktree once, fills the services
        // map. Cheap (a few fs reads per worktree); done in background so the
        // window opens fast even with 28 worktrees.
        {
            let services = services.clone();
            let worktrees_clone = worktrees.clone();
            let ctx = cc.egui_ctx.clone();
            thread::spawn(move || {
                for w in &worktrees_clone {
                    let detected = detect_services(&w.path);
                    let mut map = services.lock().unwrap();
                    map.insert(w.path.clone(), detected);
                    drop(map);
                    ctx.request_repaint();
                }
            });
        }

        // Reaper: every 2s, sweep active_runs for processes whose PGID is gone
        // (e.g. a dev server that crashed or was killed externally) and drop them
        // so the UI returns to "idle" state.
        {
            let active_runs = active_runs.clone();
            let ctx = cc.egui_ctx.clone();
            thread::spawn(move || loop {
                std::thread::sleep(Duration::from_secs(2));
                let mut dead = Vec::new();
                {
                    let map = active_runs.lock().unwrap();
                    for (pgid, _) in map.iter() {
                        let rc = unsafe { libc::kill(-*pgid, 0) };
                        if rc != 0 {
                            let errno = std::io::Error::last_os_error().raw_os_error();
                            if errno == Some(libc::ESRCH) {
                                dead.push(*pgid);
                            }
                        }
                    }
                }
                if !dead.is_empty() {
                    let mut map = active_runs.lock().unwrap();
                    for pgid in &dead {
                        map.remove(pgid);
                    }
                    drop(map);
                    ctx.request_repaint();
                }
            });
        }

        Self {
            repos,
            worktrees,
            meta,
            services,
            active_runs,
            state,
            scanner_kick,
            probe_kick,
            view: ViewMode::Listeners,
            show_only_managed: false,
            filter: String::new(),
            confirm_kill_all: false,
            expanded_repos: BTreeSet::new(),
            wt_filter: String::new(),
            server_filter: String::new(),
            browser_choice: 0,
            server_msg,
        }
    }

    fn spawn_kill(&self, pgid: i32, ctx: &egui::Context) {
        let state = self.state.clone();
        let kick = self.scanner_kick.clone();
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
        let kick = self.scanner_kick.clone();
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

    fn kick_probe(&self) {
        let (lock, cvar) = &*self.probe_kick;
        let _g = lock.lock().unwrap();
        cvar.notify_all();
    }

    fn browser_app_name(&self) -> Option<&'static str> {
        if self.browser_choice == 0 {
            None
        } else {
            BROWSER_APP_NAMES.get(self.browser_choice - 1).copied()
        }
    }

    fn spawn_start(&self, worktree_path: PathBuf, service: DetectedService, ctx: &egui::Context) {
        let active_runs = self.active_runs.clone();
        let server_msg = self.server_msg.clone();
        let scanner_kick = self.scanner_kick.clone();
        let ctx = ctx.clone();
        thread::spawn(move || {
            let log_root = std::env::temp_dir().join("hive-logs");
            let ts = chrono::Utc::now().format("%Y%m%d-%H%M%S").to_string();
            let safe_name: String = service
                .name
                .chars()
                .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
                .collect();
            let log_path = log_root.join(format!("{ts}-{safe_name}.log"));
            let cwd = worktree_path.join(&service.cwd_rel);
            match spawn_in_session(&service.command, &cwd, &log_path) {
                Ok(run) => {
                    let active = ActiveRun {
                        worktree_path: worktree_path.clone(),
                        service_name: service.name.clone(),
                        command: service.command.clone(),
                        pid: run.pid,
                        pgid: run.pgid,
                        started_at: Instant::now(),
                        log_path: run.log_path,
                    };
                    active_runs.lock().unwrap().insert(run.pgid, active);
                    *server_msg.lock().unwrap() =
                        Some(format!("started '{}' (pid {})", service.name, run.pid));
                    // Wake the scanner so the new listeners appear ASAP.
                    let (lock, cvar) = &*scanner_kick;
                    let _g = lock.lock().unwrap();
                    cvar.notify_all();
                }
                Err(e) => {
                    *server_msg.lock().unwrap() =
                        Some(format!("spawn failed for '{}': {}", service.name, e));
                }
            }
            ctx.request_repaint();
        });
    }

    fn spawn_stop_run(&self, pgid: i32, service_name: String, ctx: &egui::Context) {
        let active_runs = self.active_runs.clone();
        let server_msg = self.server_msg.clone();
        let scanner_kick = self.scanner_kick.clone();
        let ctx = ctx.clone();
        thread::spawn(move || {
            let msg = match kill_pgid(pgid, Duration::from_secs(5)) {
                Ok(KillOutcome::Terminated) => format!("stopped '{service_name}' (SIGTERM)"),
                Ok(KillOutcome::Killed) => format!("force-killed '{service_name}' (SIGKILL)"),
                Ok(KillOutcome::NotFound) => format!("'{service_name}' already gone"),
                Err(e) => format!("stop '{service_name}' failed: {e}"),
            };
            active_runs.lock().unwrap().remove(&pgid);
            *server_msg.lock().unwrap() = Some(msg);
            let (lock, cvar) = &*scanner_kick;
            let _g = lock.lock().unwrap();
            cvar.notify_all();
            ctx.request_repaint();
        });
    }

    fn open_in_browser(&self, port: u16) {
        let url = url_for_port(port);
        let browser = self.browser_app_name();
        match open_url(&url, browser) {
            Ok(()) => {
                let label = browser.unwrap_or("default browser");
                *self.server_msg.lock().unwrap() =
                    Some(format!("opened {url} in {label}"));
            }
            Err(e) => {
                *self.server_msg.lock().unwrap() = Some(format!("open failed: {e}"));
            }
        }
    }
}

impl eframe::App for HiveApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let (rows_snapshot, last_scan, last_error, last_kill_msg, total_count, matched_count) =
            snapshot_listeners(&self.state, &self.filter, self.show_only_managed);

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
                ui.separator();
                ui.selectable_value(&mut self.view, ViewMode::Listeners, "Listeners");
                ui.selectable_value(&mut self.view, ViewMode::Worktrees, "Worktrees");
                ui.selectable_value(&mut self.view, ViewMode::Servers, "Servers");
                ui.separator();
                if let Some(at) = last_scan {
                    ui.label(format!("{}s since last scan", at.elapsed().as_secs()));
                } else {
                    ui.label("scanning…");
                }
                if let Some(err) = &last_error {
                    ui.colored_label(egui::Color32::RED, format!("error: {err}"));
                }
                ui.separator();
                ui.label(format!("{} listeners", total_count));
                ui.label(format!("({matched_count} attributed)"));

                if self.view == ViewMode::Listeners {
                    ui.separator();
                    let label = format!("Kill all in filter ({})", unique_pgids_in_filter.len());
                    let enabled = !unique_pgids_in_filter.is_empty();
                    if ui.add_enabled(enabled, egui::Button::new(label)).clicked() {
                        self.confirm_kill_all = true;
                    }
                    if let Some(msg) = &last_kill_msg {
                        ui.separator();
                        ui.label(egui::RichText::new(msg).small());
                    }
                } else if self.view == ViewMode::Worktrees {
                    ui.separator();
                    if ui.button("Re-probe git").clicked() {
                        self.kick_probe();
                    }
                } else {
                    ui.separator();
                    ui.label("Browser:");
                    let current_label = match self.browser_choice {
                        0 => "Default".to_string(),
                        i => BROWSER_APP_NAMES.get(i - 1).copied().unwrap_or("?").to_string(),
                    };
                    egui::ComboBox::from_id_salt("browser_choice_combo")
                        .selected_text(current_label)
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut self.browser_choice, 0, "Default");
                            for (i, name) in BROWSER_APP_NAMES.iter().enumerate() {
                                ui.selectable_value(&mut self.browser_choice, i + 1, *name);
                            }
                        });
                    if let Some(msg) = self.server_msg.lock().unwrap().clone() {
                        ui.separator();
                        ui.label(egui::RichText::new(msg).small());
                    }
                }
            });
            ui.horizontal(|ui| match self.view {
                ViewMode::Listeners => {
                    ui.checkbox(&mut self.show_only_managed, "only attributed to a known repo");
                    ui.label("filter:");
                    ui.text_edit_singleline(&mut self.filter);
                }
                ViewMode::Worktrees => {
                    ui.label("filter:");
                    ui.text_edit_singleline(&mut self.wt_filter);
                    ui.label(
                        egui::RichText::new("matches repo name, branch, or path")
                            .small()
                            .weak(),
                    );
                }
                ViewMode::Servers => {
                    ui.label("filter:");
                    ui.text_edit_singleline(&mut self.server_filter);
                    ui.label(
                        egui::RichText::new("matches repo, branch, service, or command")
                            .small()
                            .weak(),
                    );
                }
            });
        });

        match self.view {
            ViewMode::Listeners => self.render_listeners_view(ctx, &rows_snapshot, &unique_pgids_in_filter),
            ViewMode::Worktrees => self.render_worktrees_view(ctx),
            ViewMode::Servers => self.render_servers_view(ctx),
        }
    }
}

fn snapshot_listeners(
    state: &Arc<Mutex<ScanState>>,
    filter: &str,
    only_managed: bool,
) -> (
    Vec<AttributedListener>,
    Option<Instant>,
    Option<String>,
    Option<String>,
    usize,
    usize,
) {
    let filter_lc = filter.to_lowercase();
    let s = state.lock().unwrap();
    let rows: Vec<AttributedListener> = s
        .listeners
        .iter()
        .filter(|l| !only_managed || l.repo_name.is_some())
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
}

impl HiveApp {
    fn render_listeners_view(
        &mut self,
        ctx: &egui::Context,
        rows_snapshot: &[AttributedListener],
        unique_pgids_in_filter: &[i32],
    ) {
        egui::SidePanel::right("repos")
            .resizable(true)
            .default_width(260.0)
            .show(ctx, |ui| {
                ui.heading("Tracked repos");
                ui.add_space(2.0);
                ui.label(
                    egui::RichText::new(format!("{} worktrees", self.worktrees.len()))
                        .small()
                        .weak(),
                );
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

                    ui.horizontal(|ui| {
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
                    });

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
                    for row in rows_snapshot {
                        let l = &row.listener;
                        body.row(20.0, |mut r| {
                            r.col(|ui| { ui.label(egui::RichText::new(l.port.to_string()).monospace().strong()); });
                            r.col(|ui| { ui.label(egui::RichText::new(l.pid.to_string()).monospace()); });
                            r.col(|ui| { ui.label(egui::RichText::new(l.pgid.to_string()).monospace()); });
                            r.col(|ui| { ui.label(&l.command_name); });
                            r.col(|ui| match &row.repo_name {
                                Some(n) => { ui.colored_label(egui::Color32::from_rgb(120, 230, 140), n); }
                                None => { ui.label(egui::RichText::new("—").weak()); }
                            });
                            r.col(|ui| match &row.worktree_branch {
                                Some(b) => { ui.label(egui::RichText::new(b).small()); }
                                None => { ui.label(egui::RichText::new("—").weak()); }
                            });
                            r.col(|ui| {
                                let text = l.cwd.as_ref().map(|p| p.to_string_lossy().to_string())
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
                        pgid_count, if pgid_count == 1 { "" } else { "s" }
                    ));
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        if ui.add(
                            egui::Button::new(
                                egui::RichText::new("Confirm").color(egui::Color32::WHITE),
                            ).fill(egui::Color32::from_rgb(180, 60, 60))
                        ).clicked() { do_confirm = true; }
                        if ui.button("Cancel").clicked() { do_cancel = true; }
                    });
                });
            if do_confirm {
                self.spawn_kill_many(unique_pgids_in_filter.to_vec(), ctx);
                self.confirm_kill_all = false;
            } else if do_cancel || !open {
                self.confirm_kill_all = false;
            }
        }
    }

    fn render_worktrees_view(&mut self, ctx: &egui::Context) {
        // Pre-compute per-worktree listener counts so we don't re-lock per row.
        let listener_counts: HashMap<PathBuf, usize> = {
            let s = self.state.lock().unwrap();
            let mut counts: HashMap<PathBuf, usize> = HashMap::new();
            for l in &s.listeners {
                if let Some(p) = &l.worktree_path {
                    *counts.entry(p.clone()).or_default() += 1;
                }
            }
            counts
        };
        let meta_snapshot: HashMap<PathBuf, WorktreeMeta> = self.meta.lock().unwrap().clone();
        let wt_filter_lc = self.wt_filter.to_lowercase();

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.label(
                egui::RichText::new(format!(
                    "{} worktrees across {} repos. Click 'Re-probe git' to refresh status.",
                    self.worktrees.len(),
                    self.repos.len()
                ))
                .weak()
                .small(),
            );
            ui.add_space(4.0);

            let mut by_repo: HashMap<&str, Vec<&WorktreeRef>> = HashMap::new();
            for w in &self.worktrees {
                by_repo.entry(w.repo_name.as_str()).or_default().push(w);
            }

            egui::ScrollArea::vertical().id_salt("worktrees_outer_scroll").show(ui, |ui| {
                for repo in &self.repos {
                    let Some(wts) = by_repo.get(repo.name.as_str()) else { continue; };

                    let visible_wts: Vec<&WorktreeRef> = wts
                        .iter()
                        .copied()
                        .filter(|w| matches_wt_filter(w, &wt_filter_lc))
                        .collect();
                    if !wt_filter_lc.is_empty() && visible_wts.is_empty() {
                        continue;
                    }

                    // Wrap the entire per-repo section so every widget inside (headers,
                    // chips, the TableBuilder, and all its cells) gets a unique parent
                    // ID. Without this, the cell-level widget IDs collide across the
                    // stacked tables because they share the outer ScrollArea as parent.
                    ui.push_id(format!("repo_section_{}", repo.name), |ui| {
                    let total_listeners: usize = visible_wts
                        .iter()
                        .map(|w| listener_counts.get(&w.path).copied().unwrap_or(0))
                        .sum();
                    let dirty_count = visible_wts
                        .iter()
                        .filter(|w| meta_snapshot.get(&w.path).and_then(|m| m.dirty).unwrap_or(false))
                        .count();
                    let drifted_count = visible_wts
                        .iter()
                        .filter(|w| {
                            meta_snapshot.get(&w.path).map(|m| {
                                m.ahead.unwrap_or(0) + m.behind.unwrap_or(0) > 0
                            }).unwrap_or(false)
                        })
                        .count();

                    ui.horizontal(|ui| {
                        ui.heading(&repo.name);
                        ui.label(egui::RichText::new(format!("({} wt)", visible_wts.len())).weak());
                        ui.separator();
                        if total_listeners > 0 {
                            ui.colored_label(
                                egui::Color32::from_rgb(120, 230, 140),
                                format!("{} listening", total_listeners),
                            );
                        }
                        if dirty_count > 0 {
                            ui.colored_label(
                                egui::Color32::from_rgb(230, 180, 100),
                                format!("{} dirty", dirty_count),
                            );
                        }
                        if drifted_count > 0 {
                            ui.colored_label(
                                egui::Color32::from_rgb(180, 180, 240),
                                format!("{} drifted", drifted_count),
                            );
                        }
                    });
                    ui.label(
                        egui::RichText::new(repo.path.display().to_string())
                            .small()
                            .weak(),
                    );
                    ui.add_space(4.0);

                    TableBuilder::new(ui)
                        .id_salt(format!("wt_table_{}", repo.name))
                        .vscroll(false) // outer ScrollArea owns scrolling; per-table scroll areas would ID-collide
                        .striped(true)
                        .resizable(true)
                        .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
                        .column(Column::initial(220.0).at_least(140.0))   // branch
                        .column(Column::initial(82.0).at_least(72.0))     // head
                        .column(Column::initial(80.0).at_least(60.0))     // status
                        .column(Column::initial(88.0).at_least(68.0))     // ahead/behind
                        .column(Column::initial(96.0).at_least(78.0))     // last commit
                        .column(Column::initial(80.0).at_least(60.0))     // listeners
                        .column(Column::remainder().at_least(160.0))      // path
                        .header(22.0, |mut h| {
                            h.col(|ui| { ui.strong("BRANCH"); });
                            h.col(|ui| { ui.strong("HEAD"); });
                            h.col(|ui| { ui.strong("STATUS"); });
                            h.col(|ui| { ui.strong("↑ / ↓"); });
                            h.col(|ui| { ui.strong("LAST COMMIT"); });
                            h.col(|ui| { ui.strong("LISTENERS"); });
                            h.col(|ui| { ui.strong("PATH"); });
                        })
                        .body(|mut body| {
                            for w in &visible_wts {
                                let m = meta_snapshot.get(&w.path).cloned().unwrap_or_default();
                                let listener_n = listener_counts.get(&w.path).copied().unwrap_or(0);
                                body.row(22.0, |mut r| {
                                    r.col(|ui| {
                                        let branch_text = w.branch.clone().unwrap_or_else(|| "(detached)".into());
                                        if w.branch.is_none() {
                                            ui.label(egui::RichText::new(branch_text).italics().weak());
                                        } else {
                                            ui.label(egui::RichText::new(branch_text));
                                        }
                                    });
                                    r.col(|ui| {
                                        let head = short_head(&w.head);
                                        ui.label(egui::RichText::new(head).monospace().small());
                                    });
                                    r.col(|ui| match m.dirty {
                                        Some(true) => {
                                            ui.colored_label(egui::Color32::from_rgb(230, 180, 100), "dirty");
                                        }
                                        Some(false) => {
                                            ui.colored_label(egui::Color32::from_rgb(120, 230, 140), "clean");
                                        }
                                        None => {
                                            ui.label(egui::RichText::new("…").weak());
                                        }
                                    });
                                    r.col(|ui| {
                                        let txt = match (m.ahead, m.behind) {
                                            (Some(0), Some(0)) => "—".to_string(),
                                            (Some(a), Some(b)) => format!("↑{a} ↓{b}"),
                                            _ => "…".to_string(),
                                        };
                                        let weak = matches!(txt.as_str(), "—" | "…");
                                        if weak {
                                            ui.label(egui::RichText::new(txt).weak().small());
                                        } else {
                                            ui.label(egui::RichText::new(txt).small().monospace());
                                        }
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
                                            ui.colored_label(
                                                egui::Color32::from_rgb(120, 230, 140),
                                                format!("{listener_n}"),
                                            );
                                        } else {
                                            ui.label(egui::RichText::new("—").weak());
                                        }
                                    });
                                    r.col(|ui| {
                                        ui.label(
                                            egui::RichText::new(w.path.display().to_string())
                                                .small()
                                                .weak(),
                                        );
                                    });
                                });
                            }
                        });
                    }); // close ui.push_id

                    ui.add_space(12.0);
                }
            });
        });
    }
}

impl HiveApp {
    fn render_servers_view(&mut self, ctx: &egui::Context) {
        // Snapshot everything we need under each lock briefly so the central
        // panel render below doesn't hold them.
        let services_snapshot: HashMap<PathBuf, Vec<DetectedService>> =
            self.services.lock().unwrap().clone();
        let active_runs: HashMap<i32, ActiveRun> = self.active_runs.lock().unwrap().clone();
        let listeners: Vec<LocalListener> = {
            let s = self.state.lock().unwrap();
            s.listeners.iter().map(|al| al.listener.clone()).collect()
        };
        let filter_lc = self.server_filter.to_lowercase();

        // Index listeners by pgid for fast per-run port lookup.
        let mut ports_by_pgid: HashMap<i32, Vec<u16>> = HashMap::new();
        for l in &listeners {
            ports_by_pgid.entry(l.pgid).or_default().push(l.port);
        }
        for v in ports_by_pgid.values_mut() {
            v.sort();
            v.dedup();
        }

        // Pending UI actions, applied after the central panel renders so we
        // don't borrow self while inside its callbacks.
        let mut pending_start: Option<(PathBuf, DetectedService)> = None;
        let mut pending_stop: Option<(i32, String)> = None;
        let mut pending_open: Option<u16> = None;

        egui::CentralPanel::default().show(ctx, |ui| {
            let detected_total: usize = services_snapshot.values().map(|v| v.len()).sum();
            let known_paths: usize = services_snapshot.len();
            ui.label(
                egui::RichText::new(format!(
                    "{detected_total} services detected across {known_paths}/{wt_total} worktrees · {active} running",
                    wt_total = self.worktrees.len(),
                    active = active_runs.len(),
                ))
                .weak()
                .small(),
            );
            ui.add_space(4.0);

            // Group: by repo, then by worktree, then list services.
            // Compute visible rows up front so we can skip empty repos under a filter.
            let mut wts_by_repo: HashMap<&str, Vec<&WorktreeRef>> = HashMap::new();
            for w in &self.worktrees {
                wts_by_repo.entry(w.repo_name.as_str()).or_default().push(w);
            }

            egui::ScrollArea::vertical()
                .id_salt("servers_outer_scroll")
                .show(ui, |ui| {
                    for repo in &self.repos {
                        let Some(wts) = wts_by_repo.get(repo.name.as_str()) else { continue };
                        ui.push_id(format!("server_repo_{}", repo.name), |ui| {
                            let mut any_visible = false;
                            let mut repo_running = 0usize;
                            let mut repo_services_total = 0usize;
                            // First pass for the header counts.
                            for w in wts.iter() {
                                let svcs = services_snapshot.get(&w.path).cloned().unwrap_or_default();
                                repo_services_total += svcs.len();
                                for s in &svcs {
                                    if active_runs
                                        .values()
                                        .any(|r| r.worktree_path == w.path && r.service_name == s.name)
                                    {
                                        repo_running += 1;
                                    }
                                }
                            }

                            ui.horizontal(|ui| {
                                ui.heading(&repo.name);
                                ui.label(
                                    egui::RichText::new(format!(
                                        "({} svc · {} wt)",
                                        repo_services_total, wts.len()
                                    ))
                                    .weak(),
                                );
                                if repo_running > 0 {
                                    ui.separator();
                                    ui.colored_label(
                                        egui::Color32::from_rgb(120, 230, 140),
                                        format!("{repo_running} running"),
                                    );
                                }
                            });
                            ui.add_space(2.0);

                            TableBuilder::new(ui)
                                .id_salt(format!("server_table_{}", repo.name))
                                .vscroll(false)
                                .striped(true)
                                .resizable(true)
                                .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
                                .column(Column::initial(220.0).at_least(140.0))   // worktree (branch)
                                .column(Column::initial(180.0).at_least(120.0))   // service
                                .column(Column::initial(240.0).at_least(160.0))   // command
                                .column(Column::initial(96.0).at_least(72.0))     // state
                                .column(Column::initial(120.0).at_least(70.0))    // ports
                                .column(Column::remainder().at_least(170.0))      // actions
                                .header(22.0, |mut h| {
                                    h.col(|ui| { ui.strong("WORKTREE"); });
                                    h.col(|ui| { ui.strong("SERVICE"); });
                                    h.col(|ui| { ui.strong("COMMAND"); });
                                    h.col(|ui| { ui.strong("STATE"); });
                                    h.col(|ui| { ui.strong("PORTS"); });
                                    h.col(|ui| { ui.strong("ACTIONS"); });
                                })
                                .body(|mut body| {
                                    for w in wts {
                                        let branch =
                                            w.branch.clone().unwrap_or_else(|| "(detached)".into());
                                        let svcs = services_snapshot
                                            .get(&w.path)
                                            .cloned()
                                            .unwrap_or_default();
                                        if svcs.is_empty() {
                                            // Still useful to surface "no services detected" if the user
                                            // is searching for a worktree. Skip otherwise to reduce noise.
                                            if filter_lc.is_empty() {
                                                continue;
                                            }
                                            let row_text = format!(
                                                "{} {} {}",
                                                w.repo_name,
                                                branch,
                                                w.path.display()
                                            );
                                            if !row_text.to_lowercase().contains(&filter_lc) {
                                                continue;
                                            }
                                            any_visible = true;
                                            body.row(20.0, |mut r| {
                                                r.col(|ui| { ui.label(&branch); });
                                                r.col(|ui| { ui.label(egui::RichText::new("(none detected)").weak()); });
                                                r.col(|_| {});
                                                r.col(|_| {});
                                                r.col(|_| {});
                                                r.col(|_| {});
                                            });
                                            continue;
                                        }

                                        for svc in &svcs {
                                            let row_text = format!(
                                                "{} {} {} {}",
                                                w.repo_name, branch, svc.name, svc.command
                                            );
                                            if !filter_lc.is_empty()
                                                && !row_text.to_lowercase().contains(&filter_lc)
                                            {
                                                continue;
                                            }
                                            any_visible = true;
                                            let run_for_this = active_runs.values().find(|r| {
                                                r.worktree_path == w.path && r.service_name == svc.name
                                            });

                                            body.row(22.0, |mut r| {
                                                r.col(|ui| {
                                                    ui.label(&branch);
                                                });
                                                r.col(|ui| {
                                                    ui.label(&svc.name);
                                                });
                                                r.col(|ui| {
                                                    ui.label(
                                                        egui::RichText::new(&svc.command)
                                                            .small()
                                                            .monospace(),
                                                    );
                                                });
                                                r.col(|ui| match run_for_this {
                                                    Some(run) => {
                                                        let uptime = run.started_at.elapsed();
                                                        let uptime_s = uptime.as_secs();
                                                        let uptime_str = if uptime_s < 60 {
                                                            format!("{uptime_s}s")
                                                        } else if uptime_s < 3600 {
                                                            format!("{}m", uptime_s / 60)
                                                        } else {
                                                            format!("{}h", uptime_s / 3600)
                                                        };
                                                        ui.colored_label(
                                                            egui::Color32::from_rgb(120, 230, 140),
                                                            format!(
                                                                "running · pid {} · {}",
                                                                run.pid, uptime_str
                                                            ),
                                                        );
                                                    }
                                                    None => {
                                                        ui.label(egui::RichText::new("idle").weak());
                                                    }
                                                });
                                                r.col(|ui| match run_for_this {
                                                    Some(run) => {
                                                        let ports = ports_by_pgid
                                                            .get(&run.pgid)
                                                            .cloned()
                                                            .unwrap_or_default();
                                                        if ports.is_empty() {
                                                            ui.label(
                                                                egui::RichText::new("…")
                                                                    .weak(),
                                                            );
                                                        } else {
                                                            let txt = ports
                                                                .iter()
                                                                .map(|p| p.to_string())
                                                                .collect::<Vec<_>>()
                                                                .join(", ");
                                                            ui.label(
                                                                egui::RichText::new(txt).monospace().small(),
                                                            );
                                                        }
                                                    }
                                                    None => {
                                                        ui.label(egui::RichText::new("—").weak());
                                                    }
                                                });
                                                r.col(|ui| match run_for_this {
                                                    Some(run) => {
                                                        if ui
                                                            .add(
                                                                egui::Button::new("Stop")
                                                                    .fill(egui::Color32::from_rgb(
                                                                        180, 60, 60,
                                                                    )),
                                                            )
                                                            .clicked()
                                                        {
                                                            pending_stop = Some((
                                                                run.pgid,
                                                                svc.name.clone(),
                                                            ));
                                                        }
                                                        let ports = ports_by_pgid
                                                            .get(&run.pgid)
                                                            .cloned()
                                                            .unwrap_or_default();
                                                        let openable = !ports.is_empty();
                                                        let open_label = if let Some(p) = ports.first()
                                                        {
                                                            format!("Open :{p}")
                                                        } else {
                                                            "Open".into()
                                                        };
                                                        if ui
                                                            .add_enabled(
                                                                openable,
                                                                egui::Button::new(open_label),
                                                            )
                                                            .clicked()
                                                        {
                                                            if let Some(p) = ports.first() {
                                                                pending_open = Some(*p);
                                                            }
                                                        }
                                                    }
                                                    None => {
                                                        if ui.button("Start").clicked() {
                                                            pending_start =
                                                                Some((w.path.clone(), svc.clone()));
                                                        }
                                                    }
                                                });
                                            });
                                        }
                                    }
                                });

                            // If the user filtered everything out, drop the per-repo
                            // heading completely so the view doesn't show empty sections.
                            let _ = any_visible;
                            ui.add_space(10.0);
                        });
                    }
                });
        });

        if let Some((path, svc)) = pending_start {
            self.spawn_start(path, svc, ctx);
        }
        if let Some((pgid, name)) = pending_stop {
            self.spawn_stop_run(pgid, name, ctx);
        }
        if let Some(port) = pending_open {
            self.open_in_browser(port);
        }
    }
}

fn matches_wt_filter(w: &WorktreeRef, filter_lc: &str) -> bool {
    if filter_lc.is_empty() {
        return true;
    }
    w.repo_name.to_lowercase().contains(filter_lc)
        || w.branch.as_deref().unwrap_or("").to_lowercase().contains(filter_lc)
        || w.path.to_string_lossy().to_lowercase().contains(filter_lc)
}

fn short_head(sha: &str) -> &str {
    if sha.len() >= 8 { &sha[..8] } else { sha }
}

