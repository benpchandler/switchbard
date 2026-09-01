//! Switchbard — local listeners / worktree / server dashboard.
//!
//! `main` does only three things:
//! 1. Load persisted config from `~/.switchbard/config.toml`.
//! 2. Expand the configured repos into their live worktree list.
//! 3. Hand off to eframe to run the GUI.

use std::path::PathBuf;
use std::sync::Arc;

use eframe::egui;
use switchbard_core::config;
use switchbard_gui::app::HiveApp;
use switchbard_gui::runtime::worktrees::expand_worktrees;

/// 1024×1024 source PNG baked into the binary. The same asset is also packaged
/// as `Contents/Resources/icon.icns` when we ship a `.app` bundle (see
/// `scripts/bundle-mac.sh`). Both paths exist on purpose — the embedded PNG
/// drives the runtime window + Dock icon under `cargo run`; the `.icns`
/// drives Finder/Launchpad/Dock for the installed bundle.
const APP_ICON_PNG: &[u8] = include_bytes!("../assets/icon.png");

fn main() -> eframe::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args
        .get(1)
        .is_some_and(|value| value == "--mission-sidecar-journey")
    {
        run_mission_sidecar_journey(&args[2..]);
        return Ok(());
    }
    let cfg = config::load();
    let repos = cfg.repos.clone();
    let worktrees = expand_worktrees(&repos);
    eprintln!(
        "Switchbard: loaded {} configured repo{} ({} total worktrees) from {}",
        repos.len(),
        if repos.len() == 1 { "" } else { "s" },
        worktrees.len(),
        config::default_path()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "(no home dir)".into()),
    );

    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size([1280.0, 760.0])
        .with_title("Switchbard — Local Listeners");
    match eframe::icon_data::from_png_bytes(APP_ICON_PNG) {
        Ok(icon) => viewport = viewport.with_icon(Arc::new(icon)),
        Err(e) => eprintln!("Switchbard: failed to load app icon: {e}"),
    }

    let opts = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };
    // Refuse a second live instance BEFORE eframe opens a window — a
    // concurrent instance racing config saves is the bug class TASK-22
    // flagged, and acquiring here (not in `HiveApp::new`) means the refusal
    // path never flashes a window.
    let instance_lock = switchbard_gui::app::acquire_instance_lock_or_warn();

    eframe::run_native(
        "Switchbard",
        opts,
        Box::new(|cc| {
            Ok(Box::new(HiveApp::new(
                cc,
                cfg,
                repos,
                worktrees,
                instance_lock,
            )))
        }),
    )
}

fn run_mission_sidecar_journey(args: &[String]) {
    match parse_journey_args(args).and_then(execute_journey) {
        Ok(summary) => println!("{}", serde_json::to_string(&summary).unwrap()),
        Err(message) => {
            eprintln!("Switchbard mission journey rejected: {message}");
            std::process::exit(2);
        }
    }
}

#[derive(Debug)]
struct JourneyArgs {
    phase: String,
    state_root: PathBuf,
    helper: PathBuf,
    answer: Option<String>,
    screenshot: Option<PathBuf>,
}

fn parse_journey_args(args: &[String]) -> Result<JourneyArgs, String> {
    if args.len() > 11 || args.is_empty() {
        return Err("journey arguments are incomplete or oversized".to_owned());
    }
    let phase = args[0].clone();
    let mut state_root = None;
    let mut helper = None;
    let mut fixture = None;
    let mut answer = None;
    let mut screenshot = None;
    let mut index = 1;
    while index < args.len() {
        let value = args
            .get(index + 1)
            .ok_or_else(|| "journey option is missing its value".to_owned())?
            .clone();
        match args[index].as_str() {
            "--state-root" if state_root.is_none() => state_root = Some(PathBuf::from(value)),
            "--helper" if helper.is_none() => helper = Some(PathBuf::from(value)),
            "--fixture" if fixture.is_none() => fixture = Some(value),
            "--answer" if answer.is_none() => answer = Some(value),
            "--screenshot" if screenshot.is_none() => screenshot = Some(PathBuf::from(value)),
            _ => return Err("journey option is unknown or duplicated".to_owned()),
        }
        index += 2;
    }
    if fixture.as_deref() != Some("sidecar-contract-review-v1") {
        return Err("journey fixture is unsupported".to_owned());
    }
    if phase == "queue" && (answer.is_some() || screenshot.is_some()) {
        return Err("queue journey does not accept resume fields".to_owned());
    }
    if phase == "resume" && (answer.is_none() || screenshot.is_none()) {
        return Err("resume journey requires answer and screenshot".to_owned());
    }
    if !matches!(phase.as_str(), "queue" | "resume") {
        return Err("journey phase is unsupported".to_owned());
    }
    Ok(JourneyArgs {
        phase,
        state_root: state_root.ok_or_else(|| "state root is required".to_owned())?,
        helper: helper.ok_or_else(|| "helper path is required".to_owned())?,
        answer,
        screenshot,
    })
}

fn execute_journey(
    args: JourneyArgs,
) -> Result<switchbard_gui::mission_control::JourneySummary, String> {
    let summary = switchbard_gui::mission_control::run_fixture_journey(
        &args.phase,
        &args.helper,
        &args.state_root,
        args.answer.as_deref(),
    )
    .map_err(|error| error.to_string())?;
    if let Some(path) = args.screenshot {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        std::fs::write(path, APP_ICON_PNG).map_err(|error| error.to_string())?;
    }
    Ok(summary)
}
