//! Bounded line prompts; EOF cancels rather than accepting suggestions.
use anyhow::{bail, Result};
use std::io::{BufRead, Read, Write};
use std::path::Path;
use switchbard_core::RepositorySetupOptions;

pub(super) fn choose(
    root: &Path,
    supplied: Option<RepositorySetupOptions>,
) -> Result<Option<RepositorySetupOptions>> {
    eprintln!("No SBT workspace is configured for {}. Setup registers it in Switchbard's centralized database.", root.display());
    eprintln!(
        "Next you will choose task naming and workflow stages; nothing is saved until you confirm."
    );
    if !yes(prompt("Set it up now? [y/N] ")?) {
        return Ok(None);
    }
    let suggested = supplied.unwrap_or_default();
    explain(&suggested);
    match prompt("Use these settings [Enter], customize [c], or cancel [n]? ")? {
        Some(answer) if answer.is_empty() => Ok(Some(suggested)),
        Some(answer) if answer.eq_ignore_ascii_case("c") => custom(suggested),
        _ => Ok(None),
    }
}

fn explain(options: &RepositorySetupOptions) {
    eprintln!(
        "Task IDs identify tasks, for example {}-1. The prefix groups this repository's IDs.",
        options.task_prefix
    );
    eprintln!(
        "Statuses are work stages: {}. The first label is where new tasks start.",
        options.statuses.join(" → ")
    );
    eprintln!("Done marks a task finished; other labels are work stages.");
}

fn custom(mut options: RepositorySetupOptions) -> Result<Option<RepositorySetupOptions>> {
    let Some(prefix) = custom_prefix(&options)? else {
        return Ok(None);
    };
    options.task_prefix = prefix;
    let Some(statuses) = custom_stages(&options)? else {
        return Ok(None);
    };
    options.statuses = statuses;
    let options = options.validated()?;
    explain(&options);
    eprintln!(
        "New tasks will have IDs like {}-1 and start in {}.",
        options.task_prefix, options.statuses[0]
    );
    Ok(yes(prompt("Create this workspace with these settings? [y/N] ")?).then_some(options))
}

fn custom_prefix(options: &RepositorySetupOptions) -> Result<Option<String>> {
    corrected(
        "Task ID prefix (e.g. IW gives IW-1)",
        &options.task_prefix,
        |value| {
            let mut candidate = options.clone();
            candidate.task_prefix = value.into();
            candidate.validated().map(|_| ())
        },
    )
}

fn custom_stages(options: &RepositorySetupOptions) -> Result<Option<Vec<String>>> {
    for _ in 0..3 {
        let label = format!(
            "Workflow stages, comma-separated (first is initial; e.g. Inbox, Review, Done) [{}]: ",
            options.statuses.join(", ")
        );
        let Some(answer) = prompt(&label)? else {
            return Ok(None);
        };
        if answer.is_empty() {
            return Ok(Some(options.statuses.clone()));
        }
        let mut candidate = options.clone();
        candidate.statuses = answer.split(',').map(str::to_string).collect();
        match candidate.validated() {
            Ok(candidate) => return Ok(Some(candidate.statuses)),
            Err(error) => eprintln!("{error}. Please try again."),
        }
    }
    bail!("setup canceled after three invalid entries; resume with sbt init")
}

fn corrected(
    label: &str,
    default: &str,
    validate: impl Fn(&str) -> Result<()>,
) -> Result<Option<String>> {
    for _ in 0..3 {
        let Some(answer) = prompt(&format!("{label} [{default}]: "))? else {
            return Ok(None);
        };
        let answer = if answer.is_empty() {
            default.to_string()
        } else {
            answer
        };
        match validate(&answer) {
            Ok(()) => return Ok(Some(answer)),
            Err(error) => eprintln!("{error}. Please try again."),
        }
    }
    bail!("setup canceled after three invalid entries; resume with sbt init")
}

fn prompt(label: &str) -> Result<Option<String>> {
    let mut output = std::io::stderr().lock();
    write!(output, "{label}")?;
    output.flush()?;
    let mut line = String::new();
    let read = std::io::stdin().lock().take(2048).read_line(&mut line)?;
    if read == 0 || !line.ends_with('\n') {
        return Ok(None);
    }
    if line.len() >= 2048 {
        bail!("setup input is too long; resume with sbt init");
    }
    Ok(Some(line.trim().to_string()))
}

fn yes(answer: Option<String>) -> bool {
    answer.is_some_and(|answer| matches!(answer.to_ascii_lowercase().as_str(), "y" | "yes"))
}
