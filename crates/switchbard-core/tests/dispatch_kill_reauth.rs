//! TASK-43 audit N1: the Kill button must re-authenticate on the thread that
//! signals, not trust the pgid cached on a `DispatchRun`.
//!
//! The cached verdict is refreshed by the GUI's backlog worker every 30
//! seconds, ×8 while the window is unfocused — up to ~4 minutes stale. In that
//! window an agent can exit and the OS can reissue its process group id. A
//! Kill built on the cached number would then signal whatever inherited it,
//! which is the exact failure the sidecar's authentication exists to prevent.
//!
//! These drive `kill_dispatch_run` against **real** process groups, because
//! the claim under test is about the process table and a fabricated pgid would
//! prove nothing about it.

use std::path::PathBuf;
use std::time::Duration;

use switchbard_core::dispatch_inspect::{now_unix, DispatchRunLiveness};
use switchbard_core::{
    dispatch_log_dir, dispatch_log_stem, dispatch_pid_path, kill_dispatch_run, kill_pgid,
    spawn_in_session, wait_for_exit, DispatchKillOutcome, KillOutcome, KillRefusal,
    SIDECAR_VERSION,
};

/// A real dispatch run on disk: a live child in its own session, a log named
/// by the pipeline's stem convention, and a current-format sidecar. Tears
/// everything down on drop so a failing assertion cannot leak a `sleep`.
struct LiveRun {
    task_id: String,
    started: u64,
    pgid: i32,
    pid: u32,
    log_path: PathBuf,
    prompt_path: PathBuf,
    pid_path: PathBuf,
}

impl LiveRun {
    fn spawn() -> Self {
        let task_id = unique_task_id();
        let started = now_unix();
        let log_dir = dispatch_log_dir();
        std::fs::create_dir_all(&log_dir).unwrap();
        let stem = dispatch_log_stem(&task_id, started);
        let log_path = log_dir.join(format!("{stem}.log"));
        let prompt_path = log_dir.join(format!("{stem}-prompt.md"));
        std::fs::write(&prompt_path, "fixture prompt").unwrap();

        // Mirrors what `dispatch::run_claude_headless` spawns: a shell whose
        // command line carries the prompt path — the string the identity
        // check fingerprints — kept alive as group leader by the pipeline.
        let command = format!("cat '{}' | sleep 600", prompt_path.display());
        let run = spawn_in_session(&command, &log_dir, &log_path).unwrap();
        std::thread::sleep(Duration::from_millis(150));

        let this = Self {
            pid_path: dispatch_pid_path(&task_id, started),
            task_id,
            started,
            pgid: run.pgid,
            pid: run.pid,
            log_path,
            prompt_path,
        };
        this.write_sidecar(this.pgid, std::process::id());
        this
    }

    fn write_sidecar(&self, pgid: i32, supervisor: u32) {
        let boot = switchbard_core::boot_time::boot_epoch_unix().unwrap_or(0);
        std::fs::write(
            &self.pid_path,
            format!(
                "v={SIDECAR_VERSION}\npgid={pgid}\nsupervisor={supervisor}\nstarted={}\nboot={boot}\n",
                self.started
            ),
        )
        .unwrap();
    }

    /// Kill the agent *and reap it*, so the group is genuinely gone rather
    /// than an unreaped zombie — see `kill_pgid`'s doc on why that matters.
    fn end_agent(&self) {
        let pid = self.pid;
        let reaper = std::thread::spawn(move || wait_for_exit(pid, Duration::from_secs(5)));
        let _ = kill_pgid(self.pgid, Duration::from_secs(2));
        let _ = reaper.join();
    }
}

impl Drop for LiveRun {
    fn drop(&mut self) {
        self.end_agent();
        let _ = std::fs::remove_file(&self.log_path);
        let _ = std::fs::remove_file(&self.prompt_path);
        let _ = std::fs::remove_file(&self.pid_path);
    }
}

/// `dispatch_log_dir()` is a real shared directory (`$TMPDIR/
/// switchbard-logs`), also used by a running Switchbard — every test writing
/// there needs an id no other test, run, or app can collide with.
fn unique_task_id() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    format!(
        "TASK-killreauth-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    )
}

/// N1, the finding: the agent exits between the worker's probe (which armed
/// the button) and the user's click. The re-check has to catch that and
/// **signal nothing** — the pgid may already belong to something else.
#[test]
fn a_run_that_ended_between_arm_and_click_is_refused_and_nothing_is_signalled() {
    let run = LiveRun::spawn();
    // Arm: this is the state the cached `DispatchRun` was built from.
    assert!(
        matches!(
            switchbard_core::dispatch_inspect::probe_liveness(&run.task_id, run.started),
            DispatchRunLiveness::Alive { .. }
        ),
        "precondition: the run authenticates while it is live"
    );

    // ...and then it ends, exactly as it would while the window sat
    // unfocused for a couple of worker cadences.
    run.end_agent();

    let outcome = kill_dispatch_run(&run.task_id, run.started, Duration::from_millis(200));

    assert_eq!(
        outcome,
        DispatchKillOutcome::Refused(KillRefusal::AlreadyEnded)
    );
    assert!(!outcome.signalled(), "a refusal must be inert");
    assert!(outcome.describe(&run.task_id).contains("nothing killed"));
}

/// The other half of the same window: the agent ended *and* its pgid was
/// reissued to an unrelated live process. Occupancy must not resurrect the
/// kill — this is the case where signalling the cached number would hit a
/// stranger.
#[test]
fn a_reissued_pgid_running_something_else_is_refused() {
    let run = LiveRun::spawn();
    run.end_agent();
    // Stand in for the reissued group with one that is unquestionably alive
    // and unquestionably not this run: our own.
    let ours = unsafe { libc::getpgrp() };
    run.write_sidecar(ours, std::process::id());

    let outcome = kill_dispatch_run(&run.task_id, run.started, Duration::from_millis(200));

    assert_eq!(
        outcome,
        DispatchKillOutcome::Refused(KillRefusal::AlreadyEnded),
        "a live but unrelated group is 'that run is over', not a kill target"
    );
    assert!(!outcome.signalled());
}

/// A sidecar that stopped being trustworthy between arm and click reports the
/// doubt rather than the generic "already ended" — "we can't tell" is not
/// "it's over", and the user is owed the difference.
#[test]
fn a_sidecar_that_stopped_authenticating_is_refused_with_its_reason() {
    let run = LiveRun::spawn();
    // Same live group, but now claiming a different boot.
    let boot = switchbard_core::boot_time::boot_epoch_unix().unwrap_or(0);
    std::fs::write(
        &run.pid_path,
        format!(
            "v={SIDECAR_VERSION}\npgid={}\nsupervisor={}\nstarted={}\nboot={}\n",
            run.pgid,
            std::process::id(),
            run.started,
            boot.wrapping_add(1),
        ),
    )
    .unwrap();

    let outcome = kill_dispatch_run(&run.task_id, run.started, Duration::from_millis(200));

    assert!(
        matches!(
            outcome,
            DispatchKillOutcome::Refused(KillRefusal::NotVerifiable(_))
        ),
        "{outcome:?}"
    );
    assert!(!outcome.signalled());
    // And the agent is still running, because nothing was signalled.
    assert!(
        matches!(
            switchbard_core::dispatch_inspect::probe_liveness(&run.task_id, run.started),
            DispatchRunLiveness::Unverifiable(_)
        ),
        "the refusal must not have touched the process"
    );
}

/// The happy path still works: a run that re-authenticates is killed, and the
/// outcome reports the supervision state read from the *fresh* probe.
#[test]
fn a_run_that_still_authenticates_is_killed() {
    let run = LiveRun::spawn();
    let pid = run.pid;
    // Stand in for the pipeline thread parked in `wait_for_exit`, which is
    // what reaps the child and keeps `kill_pgid` off the zombie path.
    let pipeline = std::thread::spawn(move || wait_for_exit(pid, Duration::from_secs(10)));

    let outcome = kill_dispatch_run(&run.task_id, run.started, Duration::from_secs(2));

    assert!(outcome.signalled(), "{outcome:?}");
    match outcome {
        DispatchKillOutcome::Signalled {
            pgid,
            supervised,
            outcome,
        } => {
            assert_eq!(pgid, run.pgid);
            assert!(supervised, "this process wrote the sidecar");
            assert!(
                matches!(outcome, KillOutcome::Terminated | KillOutcome::Killed),
                "{outcome:?}"
            );
        }
        other => panic!("expected a signal: {other:?}"),
    }
    let _ = pipeline.join();
}

/// An unsupervised run is still killable — it was positively identified — but
/// the outcome must say the task is left claimed, because no pipeline remains
/// to release it.
#[test]
fn an_unsupervised_run_is_killed_but_reported_as_leaving_the_task_claimed() {
    let run = LiveRun::spawn();
    // pid 1 is launchd/init: always alive, never this process.
    run.write_sidecar(run.pgid, 1);
    let pid = run.pid;
    let reaper = std::thread::spawn(move || wait_for_exit(pid, Duration::from_secs(10)));

    let outcome = kill_dispatch_run(&run.task_id, run.started, Duration::from_secs(2));

    assert!(outcome.signalled(), "{outcome:?}");
    assert!(matches!(
        outcome,
        DispatchKillOutcome::Signalled {
            supervised: false,
            ..
        }
    ));
    assert!(outcome
        .describe(&run.task_id)
        .contains("stays on `dispatching`"));
    let _ = reaper.join();
}
