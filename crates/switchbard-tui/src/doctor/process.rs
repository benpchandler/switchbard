//! Bounded, noninteractive child probes; never expose subprocess output on failure.
use std::io::Read;
use std::os::unix::process::CommandExt;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

const OUTPUT_LIMIT: u64 = 8192;

pub(super) enum ProbeError {
    Start,
    Failed,
    Timeout,
    Output,
    Cleanup,
}

impl ProbeError {
    pub(super) fn message(&self) -> &'static str {
        match self {
            Self::Start => "could not start the command",
            Self::Failed => {
                "the command returned an error (details omitted to protect credentials)"
            }
            Self::Timeout => "the command exceeded its time limit",
            Self::Output => "the command returned too much output or its stream did not close",
            Self::Cleanup => "the child command could not be cleaned up",
        }
    }
}

pub(super) fn run(mut command: Command, seconds: u64) -> Result<Vec<u8>, ProbeError> {
    command
        .process_group(0)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .env("GIT_TERMINAL_PROMPT", "0");
    let mut child = command.spawn().map_err(|_| ProbeError::Start)?;
    let stream = child.stdout.take().expect("piped stdout");
    let (sender, receiver) = mpsc::sync_channel(1);
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let result = stream
            .take(OUTPUT_LIMIT + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| ProbeError::Output)
            .and({
                if bytes.len() as u64 > OUTPUT_LIMIT {
                    Err(ProbeError::Output)
                } else {
                    Ok(bytes)
                }
            });
        if sender.send(result).is_err() { /* timeout already reported */ }
    });
    let status = wait(&mut child, seconds);
    let cleanup = cleanup(&mut child);
    let status = status?;
    cleanup?;
    if !status.success() {
        return Err(ProbeError::Failed);
    }
    receiver
        .recv_timeout(Duration::from_secs(1))
        .map_err(|_| ProbeError::Output)?
}

fn wait(child: &mut Child, seconds: u64) -> Result<ExitStatus, ProbeError> {
    let deadline = Instant::now() + Duration::from_secs(seconds);
    while Instant::now() < deadline {
        if let Some(status) = child.try_wait().map_err(|_| ProbeError::Failed)? {
            return Ok(status);
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    Err(ProbeError::Timeout)
}

fn cleanup(child: &mut Child) -> Result<(), ProbeError> {
    let group = i32::try_from(child.id()).map_err(|_| ProbeError::Cleanup)?;
    switchbard_core::kill_pgid(group, Duration::ZERO).map_err(|_| ProbeError::Cleanup)?;
    child.wait().map_err(|_| ProbeError::Cleanup)?;
    Ok(())
}
