//! Start one detached, exact-build supervisor; the core claim rejects duplicate starts.
use anyhow::{ensure, Context, Result};
use std::fs::OpenOptions;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Command, Stdio};

pub fn launch(store: &Path, run: &str, revision: u64) -> Result<()> {
    let binary = std::env::current_exe()?;
    ensure!(
        binary.file_stem().is_some_and(|s| s == "sbt"),
        "run from the sbt binary"
    );
    let logs = store
        .parent()
        .context("Inbox store has no parent")?
        .join("bug-logs");
    std::fs::create_dir_all(&logs)?;
    let log = OpenOptions::new()
        .create(true)
        .append(true)
        .open(logs.join(format!("{run}.log")))?;
    let mut child = Command::new(binary)
        .args(["run-bug", "--store"])
        .arg(store)
        .args(["--run", run])
        .stdin(Stdio::null())
        .stdout(log.try_clone()?)
        .stderr(log)
        .process_group(0)
        .spawn()
        .context("start bug supervisor")?;
    let database = store.to_path_buf();
    let id = run.to_owned();
    std::thread::spawn(move || {
        let outcome = child.wait();
        // A successful OS spawn is not a durable claim. Surface early exits too.
        let saved = switchbard_core::bug_run::Store::open(&database).and_then(|mut store| {
            let run = store.get(&id)?;
            if run.state.is_queued() && run.revision == revision {
                store.fail_launch(
                    &id,
                    run.revision,
                    &format!("Supervisor exited before claiming the run: {outcome:?}. Use :retry."),
                )?;
            }
            Ok(())
        });
        if let Err(error) = saved {
            eprintln!("bug supervisor exit reconciliation: {error}");
        }
    });
    Ok(())
}
