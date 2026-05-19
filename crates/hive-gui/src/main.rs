use eframe::egui;
use egui_extras::{Column, TableBuilder};
use hive_core::{attribute, scan_listeners, AttributedListener, Repo};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

fn main() -> eframe::Result<()> {
    let repos = vec![
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

    let opts = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1100.0, 720.0])
            .with_title("Hive — Local Listeners"),
        ..Default::default()
    };
    eframe::run_native(
        "Hive",
        opts,
        Box::new(|cc| Ok(Box::new(HiveApp::new(cc, repos)))),
    )
}

struct HiveApp {
    repos: Vec<Repo>,
    state: Arc<Mutex<ScanState>>,
    show_only_managed: bool,
    filter: String,
}

#[derive(Default)]
struct ScanState {
    listeners: Vec<AttributedListener>,
    last_scan: Option<Instant>,
    last_error: Option<String>,
}

impl HiveApp {
    fn new(cc: &eframe::CreationContext<'_>, repos: Vec<Repo>) -> Self {
        let state = Arc::new(Mutex::new(ScanState::default()));
        let bg_state = state.clone();
        let bg_repos = repos.clone();
        let ctx = cc.egui_ctx.clone();
        thread::spawn(move || loop {
            let result = scan_listeners();
            let now = Instant::now();
            {
                let mut s = bg_state.lock().unwrap();
                match result {
                    Ok(listeners) => {
                        s.listeners = attribute(&listeners, &bg_repos);
                        s.last_error = None;
                    }
                    Err(e) => {
                        s.last_error = Some(e.to_string());
                    }
                }
                s.last_scan = Some(now);
            }
            ctx.request_repaint();
            thread::sleep(Duration::from_secs(3));
        });
        Self {
            repos,
            state,
            show_only_managed: false,
            filter: String::new(),
        }
    }
}

impl eframe::App for HiveApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::TopBottomPanel::top("top").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("Hive");
                ui.label(egui::RichText::new("local listeners").weak());
                ui.separator();
                let s = self.state.lock().unwrap();
                if let Some(at) = s.last_scan {
                    let secs = at.elapsed().as_secs();
                    ui.label(format!("{}s since last scan", secs));
                } else {
                    ui.label("scanning…");
                }
                if let Some(err) = &s.last_error {
                    ui.colored_label(egui::Color32::RED, format!("error: {err}"));
                }
                ui.separator();
                ui.label(format!("{} listeners", s.listeners.len()));
                let matched = s.listeners.iter().filter(|l| l.repo_name.is_some()).count();
                ui.label(format!("({matched} attributed)"));
            });
            ui.horizontal(|ui| {
                ui.checkbox(&mut self.show_only_managed, "only attributed to a known repo");
                ui.label("filter:");
                ui.text_edit_singleline(&mut self.filter);
            });
        });

        egui::SidePanel::right("repos").resizable(true).default_width(220.0).show(ctx, |ui| {
            ui.heading("Tracked repos");
            ui.add_space(4.0);
            let s = self.state.lock().unwrap();
            for repo in &self.repos {
                let count = s
                    .listeners
                    .iter()
                    .filter(|l| l.repo_name.as_deref() == Some(repo.name.as_str()))
                    .count();
                ui.horizontal(|ui| {
                    let color = if count > 0 {
                        egui::Color32::from_rgb(120, 230, 140)
                    } else {
                        egui::Color32::GRAY
                    };
                    ui.colored_label(color, "●");
                    ui.label(&repo.name);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if count > 0 {
                            ui.label(egui::RichText::new(format!("{count}")).strong());
                        } else {
                            ui.label(egui::RichText::new("—").weak());
                        }
                    });
                });
                ui.label(
                    egui::RichText::new(repo.path.display().to_string())
                        .small()
                        .weak(),
                );
                ui.add_space(6.0);
            }
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            let s = self.state.lock().unwrap();
            let filter_lc = self.filter.to_lowercase();
            let rows: Vec<&AttributedListener> = s
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
                })
                .collect();

            TableBuilder::new(ui)
                .striped(true)
                .resizable(true)
                .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
                .column(Column::initial(70.0).at_least(50.0)) // port
                .column(Column::initial(70.0).at_least(50.0)) // pid
                .column(Column::initial(70.0).at_least(50.0)) // pgid
                .column(Column::initial(140.0).at_least(80.0)) // command
                .column(Column::initial(140.0).at_least(80.0)) // repo
                .column(Column::remainder().at_least(120.0)) // cwd
                .header(22.0, |mut h| {
                    h.col(|ui| { ui.strong("PORT"); });
                    h.col(|ui| { ui.strong("PID"); });
                    h.col(|ui| { ui.strong("PGID"); });
                    h.col(|ui| { ui.strong("COMMAND"); });
                    h.col(|ui| { ui.strong("REPO"); });
                    h.col(|ui| { ui.strong("CWD"); });
                })
                .body(|mut body| {
                    for row in rows {
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
                            r.col(|ui| {
                                let text = l
                                    .cwd
                                    .as_ref()
                                    .map(|p| p.to_string_lossy().to_string())
                                    .unwrap_or_else(|| "(unknown)".into());
                                ui.label(egui::RichText::new(text).small());
                            });
                        });
                    }
                });
        });
    }
}
