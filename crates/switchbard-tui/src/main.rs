use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

use anyhow::{bail, Result};
use clap::{Parser, Subcommand};
use crossterm::event::{self, Event};
use switchbard_tui::app::{App, AppPaths};
use switchbard_tui::settings;
use switchbard_tui::telemetry::{self, Telemetry};
use switchbard_tui::{config, view, views};

#[derive(Parser)]
#[command(
    name = "sbt",
    version = switchbard_core::VERSION_LINE,
    about = "Terminal UI for switchbard"
)]
struct Cli {
    /// Repository root holding a backlog/ directory (default: current directory)
    #[arg(long)]
    repo: Option<PathBuf>,
    /// Open the saved default view instead of the last session (self-restarts still resume)
    #[arg(long)]
    fresh: bool,
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Summarize the local event log: what is used, what is slow, what failed
    Stats,
    /// Print where the config and event log live
    Paths,
    /// Print the git identity of this build as `key=value` lines. The
    /// install guard reads this to refuse a downgrade; see
    /// `switchbard_core::build_identity`.
    BuildId,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Some(Command::Stats) => {
            let Some(path) = telemetry::default_log_path() else {
                bail!("no home directory");
            };
            print!("{}", telemetry::stats(&path)?);
            Ok(())
        }
        Some(Command::Paths) => {
            println!(
                "config: {}",
                config::user_config_path().unwrap_or_default().display()
            );
            println!(
                "views: {}",
                views::global_path().unwrap_or_default().display()
            );
            if let Ok(cwd) = std::env::current_dir() {
                println!(
                    "repo views: {}",
                    views::repo_path(&cli.repo.clone().unwrap_or(cwd))
                        .unwrap_or_default()
                        .display()
                );
            }
            println!(
                "settings: {}",
                settings::global_path().unwrap_or_default().display()
            );
            if let Ok(cwd) = std::env::current_dir() {
                println!(
                    "repo settings: {}",
                    settings::repo_path(&cli.repo.clone().unwrap_or(cwd))
                        .unwrap_or_default()
                        .display()
                );
            }
            println!(
                "events: {}",
                telemetry::default_log_path().unwrap_or_default().display()
            );
            Ok(())
        }
        Some(Command::BuildId) => {
            print!("{}", switchbard_core::build_id_report());
            Ok(())
        }
        None => run(cli.repo.unwrap_or(std::env::current_dir()?), cli.fresh),
    }
}

fn run(repo_root: PathBuf, fresh: bool) -> Result<()> {
    let repo_root = repo_root.canonicalize()?;
    if !switchbard_core::backlog_repo_available(&repo_root)? {
        bail!("{} has no backlog/ directory", repo_root.display());
    }
    let telemetry = match telemetry::default_log_path() {
        Some(path) => Telemetry::to_file(&path),
        None => Telemetry::in_memory(),
    };
    let mut app = App::open(
        &repo_root,
        AppPaths {
            config: config::user_config_path(),
            global_views: views::global_path(),
            repo_views: views::repo_path(&repo_root),
            global_settings: settings::global_path(),
            repo_settings: settings::repo_path(&repo_root),
            work_dir: switchbard_core::default_work_dir(),
        },
        telemetry,
    );
    app.restore_session(std::env::var(RESUME_ENV).ok().as_deref(), fresh);
    let shutdown = ShutdownSignals::register()?;
    let mut terminal = ratatui::init();
    let outcome = drive(&mut terminal, &mut app, &shutdown.requested);
    ratatui::restore();
    if let Err(error) = app.checkpoint_session() {
        eprintln!("{error}");
    }
    app.telemetry.finish();
    match outcome? {
        Exit::Quit => Ok(()),
        Exit::Restart => restart_into_new_binary(&app),
    }
}

enum Exit {
    Quit,
    Restart,
}

fn drive(
    terminal: &mut ratatui::DefaultTerminal,
    app: &mut App,
    shutdown: &AtomicBool,
) -> Result<Exit> {
    let binary = InstalledBinary::current();
    app.tick();
    let mut last_tick = Instant::now();
    while !app.should_quit && !shutdown.load(Ordering::Relaxed) {
        let started = Instant::now();
        terminal.draw(|frame| view::draw(frame, app))?;
        app.telemetry.record_render(started);
        let wait = app
            .next_blink()
            .unwrap_or(Duration::from_millis(500))
            .min(Duration::from_millis(500));
        let input_ready = match event::poll(wait) {
            Ok(ready) => ready,
            Err(_) if shutdown.load(Ordering::Relaxed) => return Ok(Exit::Quit),
            Err(error) => return Err(error.into()),
        };
        if input_ready {
            if let Event::Key(key) = event::read()? {
                app.handle_key(key);
            }
        }
        if !input_ready || last_tick.elapsed() >= Duration::from_millis(500) {
            app.tick();
            last_tick = Instant::now();
            if app.mode == switchbard_tui::app::Mode::Browse
                && !app.pr_merge.is_submitting()
                && binary.was_replaced()
            {
                app.telemetry
                    .record("self_restart", binary.path.display().to_string());
                return Ok(Exit::Restart);
            }
        }
        // Polling must not starve while keyboard input remains active.
        app.tick();
    }
    Ok(Exit::Quit)
}

const RESUME_ENV: &str = "SBT_RESUME";

/// Signal handlers only flip a flag. The normal event loop owns terminal cleanup
/// and the same final checkpoint used by an explicit quit or binary replacement.
struct ShutdownSignals {
    requested: Arc<AtomicBool>,
    registrations: Vec<signal_hook::SigId>,
}

impl ShutdownSignals {
    fn register() -> std::io::Result<Self> {
        let mut shutdown = Self {
            requested: Arc::new(AtomicBool::new(false)),
            registrations: Vec::with_capacity(3),
        };
        for signal in [
            signal_hook::consts::SIGHUP,
            signal_hook::consts::SIGTERM,
            signal_hook::consts::SIGINT,
        ] {
            shutdown.registrations.push(signal_hook::flag::register(
                signal,
                Arc::clone(&shutdown.requested),
            )?);
        }
        Ok(shutdown)
    }
}

impl Drop for ShutdownSignals {
    fn drop(&mut self) {
        for registration in self.registrations.drain(..) {
            signal_hook::low_level::unregister(registration);
        }
    }
}

/// A fresh `cargo install` swaps the file under us; re-exec so the running
/// tab is always the newest build without the user restarting anything.
struct InstalledBinary {
    path: PathBuf,
    seen: Option<SystemTime>,
}

impl InstalledBinary {
    fn current() -> InstalledBinary {
        let path = std::env::current_exe().unwrap_or_default();
        let seen = config::modified_at(&path);
        InstalledBinary { path, seen }
    }

    fn was_replaced(&self) -> bool {
        let now = config::modified_at(&self.path);
        now.is_some() && now != self.seen
    }
}

fn restart_into_new_binary(app: &App) -> Result<()> {
    let exe = std::env::current_exe()?;
    let error = std::process::Command::new(exe)
        .args(std::env::args_os().skip(1))
        .env(RESUME_ENV, app.resume_state())
        .exec();
    Err(error.into())
}
