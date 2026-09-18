//! Developer setup checks with no configuration, database, or remote writes.
mod process;
mod storage;

use anyhow::Result;
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Ok,
    Warning,
    Error,
    Skipped,
}

#[derive(Debug, Serialize)]
pub struct Check {
    pub id: &'static str,
    pub required: bool,
    pub status: Status,
    pub message: String,
    pub next_step: Option<String>,
}

impl Check {
    fn new(
        id: &'static str,
        required: bool,
        status: Status,
        message: impl Into<String>,
        next: Option<&str>,
    ) -> Self {
        Self {
            id,
            required,
            status,
            message: message.into(),
            next_step: next.map(str::to_owned),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct Report {
    pub schema_version: u32,
    pub sbt_version: &'static str,
    pub sbt_commit: &'static str,
    pub executable: Option<PathBuf>,
    pub repository: Option<PathBuf>,
    pub database: Option<PathBuf>,
    pub github_requested: bool,
    pub required_failed: bool,
    pub checks: Vec<Check>,
}

/// Print a deterministic JSON or text report. Optional integration failures never
/// fail the local-task readiness exit code (0 ready or setup needed, 1 blocked).
pub fn run(repo: Option<PathBuf>, github: bool, json: bool) -> Result<i32> {
    if !json {
        eprintln!(
            "Checking local setup{}...",
            if github {
                " and optional GitHub read access"
            } else {
                ""
            }
        );
    }
    let report = inspect(repo, github);
    if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        print_text(&report);
    }
    Ok(i32::from(report.required_failed))
}

fn inspect(repo: Option<PathBuf>, github: bool) -> Report {
    let input = repo.or_else(|| std::env::current_dir().ok());
    let root = input
        .and_then(|path| path.canonicalize().ok())
        .filter(|path| path.is_dir());
    let database = switchbard_core::storage::default_database_path().ok();
    let mut checks = vec![Check::new(
        "sbt",
        true,
        Status::Ok,
        format!("Running sbt {}", switchbard_core::VERSION_LINE),
        None,
    )];
    checks.push(Check::new(
        "repository_directory",
        true,
        if root.is_some() {
            Status::Ok
        } else {
            Status::Error
        },
        if root.is_some() {
            "Repository directory exists."
        } else {
            "Repository directory is unavailable."
        },
        root.is_none()
            .then_some("Run sbt doctor inside your repository, or pass --repo <DIR>."),
    ));
    check_sb(&mut checks);
    let git = executable("git");
    checks.push(tool_check("git", true, git.as_ref(), git_install()));
    check_repository(root.as_deref(), git.is_some(), &mut checks);
    storage::check(database.as_deref(), root.as_deref(), &mut checks);
    let gh = executable("gh");
    checks.push(tool_check(
        "gh",
        false,
        gh.as_ref(),
        "Install GitHub CLI: https://cli.github.com/ . Local tasks work without it.",
    ));
    for name in ["claude", "codex"] {
        checks.push(tool_check(name, false, executable(name).as_ref(),
            "Optional: install and sign in to your preferred coding agent separately. Local tasks work without it."));
    }
    check_github(root.as_deref(), gh.is_some(), github, &mut checks);
    let required_failed = checks
        .iter()
        .any(|check| check.required && check.status == Status::Error);
    Report {
        schema_version: 1,
        sbt_version: switchbard_core::VERSION_LINE,
        sbt_commit: switchbard_core::BUILD_COMMIT,
        executable: std::env::current_exe().ok(),
        repository: root,
        database,
        github_requested: github,
        required_failed,
        checks,
    }
}

fn executable(name: &str) -> Option<PathBuf> {
    use std::os::unix::fs::PermissionsExt;
    std::env::split_paths(&std::env::var_os("PATH")?)
        .map(|dir| dir.join(name))
        .find(|path| {
            std::fs::metadata(path)
                .is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
        })
}

fn tool_check(id: &'static str, required: bool, path: Option<&PathBuf>, next: &str) -> Check {
    match path {
        Some(path) => Check::new(
            id,
            required,
            Status::Ok,
            format!("Found at {}", path.display()),
            None,
        ),
        None => Check::new(
            id,
            required,
            if required {
                Status::Error
            } else {
                Status::Warning
            },
            format!("{id} is not executable on PATH."),
            Some(next),
        ),
    }
}

fn git_install() -> &'static str {
    if cfg!(target_os = "macos") {
        "Install Apple's command line tools with xcode-select --install, then rerun sbt doctor. See https://git-scm.com/install/mac ."
    } else {
        "Install Git with your distribution's package manager, then rerun sbt doctor. See https://git-scm.com/install/linux ."
    }
}

fn check_sb(checks: &mut Vec<Check>) {
    let Some(path) = executable("sb") else {
        checks.push(tool_check(
            "sb",
            false,
            None,
            "Reinstall the matching sb + sbt release pair; agent task commands need sb.",
        ));
        return;
    };
    let mut command = Command::new(path);
    command.arg("build-id");
    match process::run(command, 5) {
        Ok(bytes) => {
            let text = String::from_utf8_lossy(&bytes);
            let commit = text.lines().find_map(|line| line.strip_prefix("commit="));
            let matches = commit == Some(switchbard_core::BUILD_COMMIT)
                && switchbard_core::BUILD_COMMIT != "unknown";
            checks.push(Check::new("sb", false, if matches { Status::Ok } else { Status::Warning },
                if matches { "sb matches the running sbt build." } else { "sb build identity differs or cannot be verified." },
                (!matches).then_some("Check command -v sb and command -v sbt; reinstall the matching release pair and remove older PATH entries.")));
        }
        Err(error) => checks.push(Check::new(
            "sb",
            false,
            Status::Warning,
            format!("Cannot verify sb: {}", error.message()),
            Some("Reinstall the matching sb + sbt release pair, then rerun sbt doctor."),
        )),
    }
}

fn check_repository(root: Option<&Path>, git: bool, checks: &mut Vec<Check>) {
    if !git || root.is_none() {
        checks.push(Check::new(
            "git_repository",
            true,
            Status::Skipped,
            "Git repository check needs Git and a directory.",
            None,
        ));
        return;
    }
    let mut command = switchbard_core::git_cmd();
    command
        .arg("-C")
        .arg(root.expect("directory checked"))
        .args(["rev-parse", "--show-toplevel"]);
    match process::run(command, 5) {
        Ok(_) => checks.push(Check::new("git_repository", true, Status::Ok, "Git can resolve this checkout or worktree.", None)),
        Err(error) => checks.push(Check::new("git_repository", true, Status::Error,
            format!("Git could not resolve a repository: {}", error.message()),
            Some("Run sbt inside an existing Git checkout, or pass --repo <DIR>. For a new project, run git init yourself."))),
    }
}

fn gh_command(root: &Path, args: &[&str]) -> Command {
    let mut command = Command::new("gh");
    command
        .current_dir(root)
        .args(args)
        .env("GH_PROMPT_DISABLED", "1")
        .env_remove("GH_REPO");
    for variable in [
        "GIT_DIR",
        "GIT_INDEX_FILE",
        "GIT_WORK_TREE",
        "GIT_COMMON_DIR",
        "GIT_OBJECT_DIRECTORY",
        "GIT_NAMESPACE",
    ] {
        command.env_remove(variable);
    }
    command
}

fn check_github(root: Option<&Path>, gh: bool, requested: bool, checks: &mut Vec<Check>) {
    if !requested || !gh || root.is_none() {
        checks.push(Check::new("github", false, Status::Skipped,
            "GitHub authentication and repository access were not checked.",
            Some("Optional: run sbt doctor --github from your repository to verify GitHub read access.")));
        return;
    }
    let root = root.expect("directory checked");
    if let Err(error) = process::run(
        gh_command(root, &["auth", "status", "--hostname", "github.com"]),
        10,
    ) {
        checks.push(Check::new("github_auth", false, Status::Warning,
            format!("GitHub authentication is unavailable: {}", error.message()),
            Some("Run gh auth login --hostname github.com --web, then gh auth status. Organization SSO may need separate authorization. Never paste tokens into reports.")));
        return;
    }
    checks.push(Check::new(
        "github_auth",
        false,
        Status::Ok,
        "GitHub CLI reports authentication for github.com. Credentials were not printed.",
        None,
    ));
    let args = ["repo", "view", "--json", "nameWithOwner,url"];
    match process::run(gh_command(root, &args), 15) {
        Ok(bytes) if github_repository(&bytes).is_some() => {
            let repository = github_repository(&bytes).expect("canonical repository checked");
            checks.push(Check::new("github_repository", false, Status::Ok,
                "GitHub CLI can read the repository on github.com. Merge permission was not tested.", None));
            check_pull_requests(root, &repository, checks);
        }
        Ok(_) => checks.push(Check::new("github_repository", false, Status::Warning,
            "Repository response is unsupported or invalid. Switchbard supports github.com.",
            Some("Check git remote -v and gh repo view; use a github.com repository for PR features."))),
        Err(error) => checks.push(Check::new("github_repository", false, Status::Warning,
            format!("GitHub repository read failed: {}", error.message()),
            Some("Check gh repo view, repository access, organization SSO, and your network. Local tasks remain available."))),
    }
}

fn github_repository(bytes: &[u8]) -> Option<String> {
    switchbard_core::pr_list::parse_repository_identity(bytes)
        .ok()
        .map(|(name, _)| name)
}

fn check_pull_requests(root: &Path, repository: &str, checks: &mut Vec<Check>) {
    let args = [
        "pr", "list", "--repo", repository, "--state", "open", "--limit", "1", "--json", "number",
    ];
    let result = process::run(gh_command(root, &args), 15);
    let status = match result {
        Ok(bytes)
            if serde_json::from_slice::<Vec<serde_json::Value>>(&bytes).is_ok_and(|items| {
                items.len() <= 1
                    && items.iter().all(|item| {
                        item.get("number")
                            .and_then(|number| number.as_u64())
                            .is_some_and(|number| number > 0)
                    })
            }) =>
        {
            Status::Ok
        }
        _ => Status::Warning,
    };
    checks.push(Check::new("github_pull_requests", false, status,
        if status == Status::Ok { "GitHub CLI can read pull requests from the resolved repository. Checks, reviews, and merge permission were not tested." }
        else { "GitHub pull request read access could not be verified." },
        (status != Status::Ok).then_some("Check gh pr list, repository pull request permissions, organization SSO, and your network. Local tasks remain available.")));
}

fn print_text(report: &Report) {
    println!(
        "Switchbard doctor: {}",
        if report.required_failed {
            "local setup blocked"
        } else {
            "local prerequisites checked"
        }
    );
    for check in &report.checks {
        let status = match check.status {
            Status::Ok => "OK",
            Status::Warning => "WARN",
            Status::Error => "ERROR",
            Status::Skipped => "SKIP",
        };
        println!("[{status}] {}: {}", check.id, check.message);
        if let Some(next) = &check.next_step {
            println!("  Next: {next}");
        }
    }
    println!("This check changes no configuration or tasks. Rerun after fixing a reported issue.");
}
