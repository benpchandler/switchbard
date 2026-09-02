use std::path::PathBuf;
use std::time::{Duration, Instant};

use anyhow::{bail, Result};
use clap::{Parser, Subcommand};
use crossterm::event::{self, Event};
use switchbard_tui::app::App;
use switchbard_tui::telemetry::{self, Telemetry};
use switchbard_tui::{config, view};

#[derive(Parser)]
#[command(name = "sbt", about = "Terminal UI for switchbard")]
struct Cli {
    /// Repository root holding a backlog/ directory (default: current directory)
    #[arg(long)]
    repo: Option<PathBuf>,
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Summarize the local event log: what is used, what is slow, what failed
    Stats,
    /// Print where the config and event log live
    Paths,
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
                "events: {}",
                telemetry::default_log_path().unwrap_or_default().display()
            );
            Ok(())
        }
        None => run(cli.repo.unwrap_or(std::env::current_dir()?)),
    }
}

fn run(repo_root: PathBuf) -> Result<()> {
    if !switchbard_core::is_backlog_repo(&repo_root) {
        bail!("{} has no backlog/ directory", repo_root.display());
    }
    let telemetry = match telemetry::default_log_path() {
        Some(path) => Telemetry::to_file(&path),
        None => Telemetry::in_memory(),
    };
    let mut app = App::open(&repo_root, config::user_config_path(), telemetry);
    let mut terminal = ratatui::init();
    let outcome = drive(&mut terminal, &mut app);
    ratatui::restore();
    app.telemetry.finish();
    outcome
}

fn drive(terminal: &mut ratatui::DefaultTerminal, app: &mut App) -> Result<()> {
    while !app.should_quit {
        let started = Instant::now();
        terminal.draw(|frame| view::draw(frame, app))?;
        app.telemetry.record_render(started);
        if event::poll(Duration::from_millis(500))? {
            if let Event::Key(key) = event::read()? {
                app.handle_key(key);
            }
        } else {
            app.tick();
        }
    }
    Ok(())
}
