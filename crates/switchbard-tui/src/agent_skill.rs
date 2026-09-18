//! Offline agent instructions embedded in every terminal release.
use anyhow::{bail, ensure, Context, Result};
use clap::{Subcommand, ValueEnum};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

const SKILL: &str = include_str!("../../../skills/switchbard/SKILL.md");
const INTERFACE: &str = include_str!("../../../skills/switchbard/agents/openai.yaml");
const PROMPT: &str = include_str!("../../../skills/switchbard/setup-prompt.md");

#[derive(Clone, Copy, ValueEnum)]
pub enum Agent {
    Claude,
    Codex,
    Both,
}

#[derive(Subcommand)]
pub enum SkillCommand {
    /// Print the embedded skill without installing it
    Show,
    /// Install personal agent skills; preserve existing custom instructions
    Install {
        #[arg(long, value_enum, default_value = "both")]
        agent: Agent,
        /// Explicitly replace differing files; symlinks are always refused
        #[arg(long)]
        replace: bool,
    },
}

pub fn print_prompt() {
    print!("{PROMPT}");
}

pub fn run(command: SkillCommand) -> Result<()> {
    match command {
        SkillCommand::Show => print!("{SKILL}"),
        SkillCommand::Install { agent, replace } => install(agent, replace)?,
    }
    Ok(())
}

fn install(agent: Agent, replace: bool) -> Result<()> {
    let home = dirs::home_dir().context("cannot determine home directory")?;
    let files = planned_files(&home, agent);
    for (path, content) in &files {
        validate_destination(&home, path, content, replace)?;
    }
    for (path, content) in &files {
        write_file(&home, path, content)?;
        println!("Skill file ready: {}", path.display());
    }
    println!("Use /switchbard in Claude Code or $switchbard in Codex. If absent, restart your session. No hooks or credentials were changed.");
    Ok(())
}

fn planned_files(home: &Path, agent: Agent) -> Vec<(PathBuf, &'static str)> {
    let roots: &[&str] = match agent {
        Agent::Claude => &[".claude"],
        Agent::Codex => &[".agents"],
        Agent::Both => &[".claude", ".agents"],
    };
    roots
        .iter()
        .flat_map(|root| {
            let dir = home.join(root).join("skills/switchbard");
            [
                (dir.join("SKILL.md"), SKILL),
                (dir.join("agents/openai.yaml"), INTERFACE),
            ]
        })
        .collect()
}

fn validate_destination(home: &Path, path: &Path, content: &str, replace: bool) -> Result<()> {
    let relative = path
        .strip_prefix(home)
        .expect("invariant: skill stays under home");
    let mut ancestor = home.to_path_buf();
    validate_kind(&ancestor, true)?;
    for part in relative.components().take(8) {
        ancestor.push(part);
        validate_kind(&ancestor, ancestor != path)?;
    }
    if path.exists() {
        ensure!(
            replace || same_content(path, content)?,
            "{} contains custom instructions; preserved. Inspect them before choosing --replace",
            path.display()
        );
    }
    Ok(())
}

fn validate_kind(path: &Path, directory: bool) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(meta) => {
            ensure!(
                !meta.file_type().is_symlink(),
                "{} is a symlink; refusing to modify its target",
                path.display()
            );
            ensure!(
                if directory {
                    meta.is_dir()
                } else {
                    meta.is_file()
                },
                "{} is not a {}",
                path.display(),
                if directory {
                    "directory"
                } else {
                    "regular file"
                }
            );
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    Ok(())
}

fn same_content(path: &Path, content: &str) -> Result<bool> {
    if fs::metadata(path)?.len() != content.len() as u64 {
        return Ok(false);
    }
    Ok(fs::read(path)? == content.as_bytes())
}

fn write_file(home: &Path, path: &Path, content: &str) -> Result<()> {
    validate_destination(home, path, content, true)?;
    if path.exists() && same_content(path, content)? {
        return Ok(());
    }
    fs::create_dir_all(path.parent().expect("invariant: destination has parent"))?;
    validate_destination(home, path, content, true)?;
    let (scratch, mut file) = scratch_file(path)?;
    let outcome = file
        .write_all(content.as_bytes())
        .and_then(|()| file.sync_all());
    drop(file);
    let outcome = outcome.map_err(anyhow::Error::from).and_then(|()| {
        validate_destination(home, path, content, true)?;
        fs::rename(&scratch, path).context("install agent skill file")
    });
    if outcome.is_err() {
        fs::remove_file(&scratch).context("remove unsuccessful skill staging file")?;
    }
    outcome
}

fn scratch_file(path: &Path) -> Result<(PathBuf, fs::File)> {
    let parent = path.parent().expect("invariant: destination has parent");
    for attempt in 0..64 {
        let scratch = parent.join(format!(
            ".switchbard-skill-{}-{attempt}",
            std::process::id()
        ));
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&scratch)
        {
            Ok(file) => return Ok((scratch, file)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error.into()),
        }
    }
    bail!(
        "could not reserve a skill staging file; inspect {} and retry",
        parent.display()
    )
}
