//! Bounded child execution with file-backed output and descendant cleanup.
use super::{Lease, Store};
use anyhow::{bail, ensure, Context, Result};
use std::fs::{File, OpenOptions};
use std::io::Read;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

pub(super) const MAX_OUTPUT: usize = 4 * 1024 * 1024;
pub(super) enum ProcessEvent<'a> {
    Spawned(u32),
    Output(&'a str),
    Reaped,
}

#[derive(Debug)]
pub(super) struct UnreapedProcess(String);
impl std::fmt::Display for UnreapedProcess {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "process group could not be reaped: {}", self.0)
    }
}
impl std::error::Error for UnreapedProcess {}

pub(super) fn capture(mut command: Command, timeout: Duration) -> Result<String> {
    command.stdin(Stdio::null());
    capture_observing(command, timeout, |_| Ok(false))
}

pub(super) fn capture_observing(
    command: Command,
    timeout: Duration,
    observe: impl FnMut(ProcessEvent<'_>) -> Result<bool>,
) -> Result<String> {
    capture_events(command, timeout, None, observe)
}

pub(super) fn capture_guarded(
    command: Command,
    timeout: Duration,
    permit: &Path,
    input: Option<&Path>,
    observe: impl FnMut(ProcessEvent<'_>) -> Result<bool>,
) -> Result<String> {
    let wrapped = startup_command(&command, permit, input)?;
    capture_events(wrapped, timeout, Some(permit), observe)
}

pub(super) fn capture_leased(
    command: Command,
    timeout: Duration,
    store: &mut Store,
    lease: &Lease,
    marker: &str,
) -> Result<String> {
    let dir = std::env::temp_dir().join(format!("switchbard-bug-start-{}", uuid::Uuid::new_v4()));
    std::fs::DirBuilder::new().mode(0o700).create(&dir)?;
    let permit = dir.join("permit");
    store.prepare_child(lease, marker, &permit)?;
    // Retain the tiny permit receipt: absence is a recovery proof only if never deleted.
    capture_guarded(command, timeout, &permit, None, |event| {
        match event {
            ProcessEvent::Spawned(pgid) => {
                store.record_child(lease, pgid)?;
            }
            ProcessEvent::Reaped => {
                store.record_child_reaped(lease)?;
            }
            ProcessEvent::Output(_) => {}
        }
        Ok(false)
    })
}

fn startup_command(command: &Command, permit: &Path, input: Option<&Path>) -> Result<Command> {
    ensure!(!permit.try_exists()?, "startup permit already exists");
    let mut wrapper = Command::new("/bin/sh");
    wrapper.args(["-c", "permit=$1; shift; count=0; while [ \"$count\" -lt 200 ]; do if [ -f \"$permit\" ]; then exec \"$@\"; fi; count=$((count+1)); /bin/sleep 0.05; done; exit 125", "switchbard-start"])
        .arg(permit).arg(command.get_program()).args(command.get_args());
    if let Some(root) = command.get_current_dir() {
        wrapper.current_dir(root);
    }
    for (name, value) in command.get_envs() {
        if let Some(value) = value {
            wrapper.env(name, value);
        } else {
            wrapper.env_remove(name);
        }
    }
    wrapper.stdin(match input {
        Some(path) => Stdio::from(File::open(path)?),
        None => Stdio::null(),
    });
    Ok(wrapper)
}

fn capture_events(
    mut command: Command,
    timeout: Duration,
    permit: Option<&Path>,
    mut observe: impl FnMut(ProcessEvent<'_>) -> Result<bool>,
) -> Result<String> {
    // Every external program can launch Git; inherit the same discovery isolation
    // as git_cmd rather than allowing a hook environment to redirect child work.
    for (name, _) in crate::git_cmd().get_envs() {
        command.env_remove(name);
    }
    let dir = std::env::temp_dir().join(format!("switchbard-bug-process-{}", uuid::Uuid::new_v4()));
    std::fs::DirBuilder::new().mode(0o700).create(&dir)?;
    let stdout = dir.join("stdout");
    let stderr = dir.join("stderr");
    command
        .stdout(Stdio::from(private_file(&stdout)?))
        .stderr(Stdio::from(private_file(&stderr)?));
    command.process_group(0);
    let mut child = command.spawn().context("start bug run process")?;
    let outcome = observe(ProcessEvent::Spawned(child.id())).and_then(|watch| {
        if let Some(path) = permit {
            private_file(path)?.sync_all()?;
        }
        wait(&mut child, &stdout, &stderr, timeout, watch, &mut observe)
    });
    let cleanup = reap_group(&mut child);
    cleanup.map_err(|error| UnreapedProcess(error.to_string()))?;
    observe(ProcessEvent::Reaped)?;
    let output = outcome.with_context(|| format!("process logs: {}", dir.display()))?;
    std::fs::remove_dir_all(&dir)?;
    Ok(output)
}

fn wait(
    child: &mut Child,
    stdout: &Path,
    stderr: &Path,
    timeout: Duration,
    mut watch: bool,
    observe: &mut impl FnMut(ProcessEvent<'_>) -> Result<bool>,
) -> Result<String> {
    let started = Instant::now();
    let ticks = timeout.as_millis().saturating_div(50).saturating_add(2);
    let mut observed_len = 0;
    for _ in 0..ticks {
        let length = check_sizes(stdout, stderr)?;
        if watch && length != observed_len {
            watch = observe(ProcessEvent::Output(&read_bounded(stdout)?))?;
            observed_len = length;
        }
        if let Some(status) = child.try_wait()? {
            let output = read_bounded(stdout)?;
            if watch {
                observe(ProcessEvent::Output(&output))?;
            }
            ensure!(
                status.success(),
                "process exited {status}: {}",
                read_bounded(stderr)?.chars().take(4096).collect::<String>()
            );
            return Ok(output);
        }
        if started.elapsed() >= timeout {
            bail!("process timed out after {} seconds", timeout.as_secs());
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    bail!("process polling limit exceeded")
}

fn check_sizes(stdout: &Path, stderr: &Path) -> Result<u64> {
    let length = std::fs::metadata(stdout)?.len();
    ensure!(
        length <= MAX_OUTPUT as u64 && std::fs::metadata(stderr)?.len() <= MAX_OUTPUT as u64,
        "process output exceeded {MAX_OUTPUT} bytes"
    );
    Ok(length)
}

pub(super) fn read_bounded(path: &Path) -> Result<String> {
    let mut bytes = Vec::new();
    File::open(path)?
        .take((MAX_OUTPUT + 1) as u64)
        .read_to_end(&mut bytes)?;
    ensure!(bytes.len() <= MAX_OUTPUT, "process output exceeds limit");
    // A running writer can end this observation in the middle of a UTF-8 codepoint.
    // Consumers parse complete JSON lines only; the final read contains the full text.
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

fn private_file(path: &Path) -> Result<File> {
    Ok(OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)?)
}

fn reap_group(child: &mut Child) -> Result<()> {
    if child.try_wait()?.is_none() {
        child.kill()?;
        child.wait()?;
    }
    let pgid = i32::try_from(child.id()).context("child process group overflows")?;
    crate::kill::kill_pgid(pgid, Duration::from_secs(2))?;
    Ok(())
}
