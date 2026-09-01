//! Switchbard — local listeners / worktree / server dashboard.
//!
//! `main` does only three things:
//! 1. Load persisted config from `~/.switchbard/config.toml`.
//! 2. Expand the configured repos into their live worktree list.
//! 3. Hand off to eframe to run the GUI.

use std::ffi::{OsStr, OsString};
use std::io::Write;
use std::os::unix::process::ExitStatusExt;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
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
const MISSION_SIDECAR_LAUNCHER: &str = "xplan-mission-sidecar-launcher";
const MISSION_SIDECAR_PAYLOAD: &str = "Resources/xplan-mission-sidecar/xplan-mission-sidecar";

fn main() -> eframe::Result<()> {
    let launcher_args: Vec<OsString> = std::env::args_os().collect();
    if launcher_args
        .first()
        .is_some_and(|argv0| is_mission_sidecar_launcher(argv0))
    {
        std::process::exit(run_mission_sidecar_launcher(&launcher_args[1..]));
    }
    let args: Vec<String> = std::env::args().collect();
    if args
        .get(1)
        .is_some_and(|value| value == "--mission-sidecar-journey")
    {
        run_mission_sidecar_journey(&args[2..]);
        return Ok(());
    }
    run_gui()
}

fn run_gui() -> eframe::Result<()> {
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

fn is_mission_sidecar_launcher(argv0: &OsStr) -> bool {
    Path::new(argv0).file_name() == Some(OsStr::new(MISSION_SIDECAR_LAUNCHER))
}

fn run_mission_sidecar_launcher(args: &[OsString]) -> i32 {
    let state_root = match parse_mission_sidecar_launcher_args(args) {
        Ok(path) => path,
        Err(message) => {
            eprintln!("Switchbard mission launcher rejected: {message}");
            return 2;
        }
    };
    let executable = match std::env::current_exe() {
        Ok(path) => path,
        Err(error) => {
            eprintln!("Switchbard mission launcher unavailable: {error}");
            return 127;
        }
    };
    match proxy_mission_sidecar(&executable, state_root) {
        Ok(status) => child_exit_code(status),
        Err(error) => {
            eprintln!("Switchbard mission launcher unavailable: {error}");
            127
        }
    }
}

fn parse_mission_sidecar_launcher_args(args: &[OsString]) -> Result<&OsStr, &'static str> {
    if args.len() != 2 || args[0] != OsStr::new("--state-root") || args[1].is_empty() {
        return Err("expected exactly --state-root <path>");
    }
    Ok(&args[1])
}

fn proxy_mission_sidecar(
    launcher: &Path,
    state_root: &OsStr,
) -> Result<ExitStatus, std::io::Error> {
    let contents = launcher
        .parent()
        .filter(|path| path.file_name() == Some(OsStr::new("Helpers")))
        .and_then(Path::parent)
        .ok_or_else(|| std::io::Error::other("launcher is outside Contents/Helpers"))?;
    let payload = contents.join(MISSION_SIDECAR_PAYLOAD);
    Command::new(payload)
        .arg("--state-root")
        .arg(state_root)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
}

fn child_exit_code(status: ExitStatus) -> i32 {
    status
        .code()
        .unwrap_or_else(|| status.signal().map_or(1, |signal| 128 + signal))
}

fn run_mission_sidecar_journey(args: &[String]) {
    match parse_journey_args(args) {
        Ok(arguments) => match execute_journey(&arguments) {
            Ok(summary) => {
                println!("{}", serde_json::to_string(&summary).unwrap());
                if std::io::stdout().flush().is_err() {
                    eprintln!("Switchbard mission journey rejected: stdout flush failed");
                    std::process::exit(2);
                }
                if let Some(screenshot) = arguments.screenshot.as_deref() {
                    if let Err(error) =
                        render_acknowledged_journey(&arguments.state_root, screenshot)
                    {
                        eprintln!("Switchbard mission journey rejected: {error}");
                        std::process::exit(2);
                    }
                }
            }
            Err(message) => {
                eprintln!("Switchbard mission journey rejected: {message}");
                std::process::exit(2);
            }
        },
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
        return Err("resume journey requires an answer and screenshot".to_owned());
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
    args: &JourneyArgs,
) -> Result<switchbard_gui::mission_control::JourneySummary, String> {
    switchbard_gui::mission_control::run_fixture_journey(
        &args.phase,
        &args.helper,
        &args.state_root,
        args.answer.as_deref(),
    )
    .map_err(|error| error.to_string())
}

fn render_acknowledged_journey(state_root: &Path, screenshot: &Path) -> Result<(), String> {
    let parent = screenshot
        .parent()
        .ok_or_else(|| "screenshot path has no parent".to_owned())?;
    std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    std::env::set_var("EFRAME_SCREENSHOT_TO", screenshot);
    std::env::set_var(
        switchbard_core::mission_projection::MISSION_PROJECTION_ENV,
        state_root.join("mission-command-snapshot.json"),
    );
    let cfg = config::load();
    let repos = cfg.repos.clone();
    let worktrees = expand_worktrees(&repos);
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 760.0])
            .with_title("Switchbard Mission acknowledged"),
        ..Default::default()
    };
    eframe::run_native(
        "Switchbard",
        options,
        Box::new(move |cc| {
            let mut app = HiveApp::new(cc, cfg, repos, worktrees, None);
            app.place = switchbard_gui::runtime::Place::Missions;
            let mut model = app.mission_control.lock().expect("mission control lock");
            model.helper_health = switchbard_gui::mission_control::HelperHealth::Ready;
            model.projection_freshness =
                switchbard_core::ProjectionFreshness::Fresh { age_seconds: 0 };
            model.request_outcome =
                switchbard_gui::mission_control::RequestOutcome::DecisionAcknowledged {
                    command_id: "fixture:resume".to_owned(),
                    accepted_revision: 2,
                };
            drop(model);
            Ok(Box::new(app))
        }),
    )
    .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packaged_launcher_identity_and_layout_are_exact() {
        assert!(is_mission_sidecar_launcher(OsStr::new(
            "/Applications/Switchbard.app/Contents/Helpers/xplan-mission-sidecar-launcher"
        )));
        assert!(!is_mission_sidecar_launcher(OsStr::new(
            "/Applications/Switchbard.app/Contents/MacOS/Switchbard"
        )));
        let launcher = Path::new(
            "/Applications/Switchbard.app/Contents/Helpers/xplan-mission-sidecar-launcher",
        );
        let error = proxy_mission_sidecar(launcher, OsStr::new("/absent-state"))
            .expect_err("fixture has no packaged payload");
        assert_eq!(error.kind(), std::io::ErrorKind::NotFound);
    }

    #[test]
    fn packaged_launcher_accepts_only_state_root() {
        let valid = [OsString::from("--state-root"), OsString::from("/tmp/state")];
        assert_eq!(
            parse_mission_sidecar_launcher_args(&valid).unwrap(),
            OsStr::new("/tmp/state")
        );
        for invalid in [
            Vec::new(),
            vec![OsString::from("--state-root")],
            vec![OsString::from("--state-root"), OsString::new()],
            vec![OsString::from("--helper"), OsString::from("payload")],
            vec![
                OsString::from("--state-root"),
                OsString::from("/tmp/state"),
                OsString::from("extra"),
            ],
        ] {
            assert!(parse_mission_sidecar_launcher_args(&invalid).is_err());
        }
    }

    #[test]
    fn packaged_journey_requires_a_real_resume_screenshot() {
        let resume = [
            "resume",
            "--state-root",
            "/tmp/state",
            "--helper",
            "/tmp/helper",
            "--fixture",
            "sidecar-contract-review-v1",
            "--answer",
            "approved",
            "--screenshot",
            "/tmp/acknowledged.png",
        ]
        .map(str::to_owned);
        let parsed = parse_journey_args(&resume).expect("complete resume journey");
        assert_eq!(
            parsed.screenshot.as_deref(),
            Some(Path::new("/tmp/acknowledged.png"))
        );
        let queue_with_screenshot = [
            "queue",
            "--state-root",
            "/tmp/state",
            "--helper",
            "/tmp/helper",
            "--fixture",
            "sidecar-contract-review-v1",
            "--screenshot",
            "/tmp/not-allowed.png",
        ]
        .map(str::to_owned);
        assert!(parse_journey_args(&queue_with_screenshot).is_err());
    }
}
