use std::io::Write;
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

use anyhow::{bail, Result};
use clap::{Parser, Subcommand};
use crossterm::event::{self, Event};
use switchbard_tui::app::{App, AppPaths};
use switchbard_tui::settings;
use switchbard_tui::telemetry::{self, Telemetry};
use switchbard_tui::{config, tty, view, views};

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
            auto_install_dir: switchbard_tui::auto_install::state_dir(),
        },
        telemetry,
    );
    let resumed = std::env::var(RESUME_ENV).ok();
    app.restore_session(resumed.as_deref(), fresh);
    let prev_build = std::env::var(switchbard_tui::auto_install::PREV_BUILD_ENV).ok();
    app.show_startup_banner(resumed.is_some(), prev_build.as_deref());
    let shutdown = ShutdownSignals::register()?;
    let hung_up = Arc::new(AtomicBool::new(false));
    tty::spawn_hangup_watch(Arc::clone(&hung_up), Arc::clone(&shutdown.requested))?;
    let mut terminal = init_terminal()?;
    let outcome = tty::spawn_event_reader()
        .map_err(anyhow::Error::from)
        .and_then(|events| {
            drive(
                &mut terminal,
                &mut app,
                &events,
                Stop {
                    requested: &shutdown.requested,
                    hung_up: &hung_up,
                },
            )
        });
    let restore = restore_terminal();
    // The cursor is already shown by `restore_terminal`; `Terminal`'s own
    // `Drop` would try again and `eprintln!` on failure, which after a
    // hangup is a panic inside the exit path. Nothing is leaked that the
    // process end does not reclaim.
    std::mem::forget(terminal);
    if let Err(error) = app.checkpoint_session() {
        // Never `eprintln!` here: after a hangup stderr is the dead pty and
        // a failed print would panic on the way out.
        let _ = writeln!(std::io::stderr(), "{error}");
    }
    app.telemetry.finish();
    // The run's own error is the one worth reporting; a failed terminal
    // restore only matters when the terminal is still ours. After a hangup
    // or a signal there may be nothing left to restore, and reporting that
    // would itself write to the dead terminal.
    let exit = outcome?;
    if !hung_up.load(Ordering::Relaxed) && !shutdown.requested.load(Ordering::Relaxed) {
        restore?;
    }
    match exit {
        Exit::Quit => Ok(()),
        Exit::Restart => restart_into_new_binary(&app),
    }
}

enum Exit {
    Quit,
    Restart,
}

/// What `ratatui::init` does - raw mode, alternate screen, a panic hook
/// that restores the terminal first - plus mouse capture, minus every
/// `eprintln!`: ratatui's hook and `Terminal::drop` print their restore
/// failures, and once the terminal is gone (TASK-232) that print is itself
/// a panic, which inside a panic hook aborts the process. The hook here
/// restores silently and then defers to the hook that was installed before.
fn init_terminal() -> std::io::Result<ratatui::DefaultTerminal> {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = restore_terminal();
        previous(info);
    }));
    crossterm::terminal::enable_raw_mode()?;
    crossterm::execute!(
        std::io::stdout(),
        crossterm::terminal::EnterAlternateScreen,
        event::EnableMouseCapture
    )?;
    ratatui::Terminal::new(ratatui::backend::CrosstermBackend::new(std::io::stdout()))
}

/// The inverse of [`init_terminal`], without ratatui's `eprintln!` on
/// failure. Every step is attempted; the first error is returned for the
/// caller to judge, since after a hangup failure is the expected outcome.
fn restore_terminal() -> std::io::Result<()> {
    let mouse = crossterm::execute!(std::io::stdout(), event::DisableMouseCapture);
    let raw = crossterm::terminal::disable_raw_mode();
    let screen = crossterm::execute!(
        std::io::stdout(),
        crossterm::terminal::LeaveAlternateScreen,
        crossterm::cursor::Show
    );
    mouse.and(raw).and(screen)
}

/// Why the loop should stop, besides the app asking to: a signal flipped
/// `requested`, or the terminal hung up. Both are observed once per loop
/// iteration, which is why the loop must never block anywhere else - see
/// `switchbard_tui::tty`.
struct Stop<'a> {
    requested: &'a AtomicBool,
    hung_up: &'a AtomicBool,
}

/// How long a failed draw waits for a stop request to explain it.
const DRAW_FAILURE_GRACE: Duration = Duration::from_millis(200);

impl Stop<'_> {
    fn asked(&self) -> bool {
        self.requested.load(Ordering::Relaxed) || self.hung_up.load(Ordering::Relaxed)
    }

    /// `asked`, re-checked every few milliseconds for up to `grace`.
    fn asked_within(&self, grace: Duration) -> bool {
        let deadline = Instant::now() + grace;
        while !self.asked() {
            if Instant::now() >= deadline {
                return false;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        true
    }
}

fn drive(
    terminal: &mut ratatui::DefaultTerminal,
    app: &mut App,
    events: &Receiver<Event>,
    stop: Stop<'_>,
) -> Result<Exit> {
    let binary = InstalledBinary::current();
    app.tick();
    let mut last_tick = Instant::now();
    while !app.should_quit && !stop.asked() {
        let started = Instant::now();
        if let Err(error) = terminal.draw(|frame| view::draw(frame, app)) {
            // A write to a terminal that has just gone away fails before
            // the hangup watch or SIGHUP has necessarily flagged it; give
            // them one bounded moment before calling it a real error.
            if stop.asked_within(DRAW_FAILURE_GRACE) {
                return Ok(Exit::Quit);
            }
            return Err(error.into());
        }
        app.telemetry.record_render(started);
        let wait = app
            .next_blink()
            .unwrap_or(Duration::from_millis(500))
            .min(Duration::from_millis(500));
        let input = match events.recv_timeout(wait) {
            Ok(event) => Some(event),
            Err(RecvTimeoutError::Timeout) => None,
            Err(RecvTimeoutError::Disconnected) if stop.asked() => return Ok(Exit::Quit),
            Err(RecvTimeoutError::Disconnected) => {
                return Err(anyhow::anyhow!("terminal event reader stopped"))
            }
        };
        let input_ready = input.is_some();
        match input {
            Some(Event::Key(key)) => app.handle_key(key),
            Some(Event::Mouse(mouse)) => app.handle_mouse(mouse),
            _ => {}
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
        .env(
            switchbard_tui::auto_install::PREV_BUILD_ENV,
            switchbard_core::BUILD_COMMIT,
        )
        .exec();
    Err(error.into())
}
