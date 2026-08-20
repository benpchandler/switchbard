//! Stopping one in-flight dispatch run, safely.
//!
//! ## Why this is not just `kill_pgid(run.pgid)`
//!
//! A `DispatchRun`'s liveness verdict is *cached*. It is recomputed by the
//! backlog worker on a 30-second cadence, multiplied by
//! `UNFOCUSED_BACKOFF_MULTIPLIER` while the window is in the background — so
//! the pgid a user is looking at when they click Kill can be minutes old.
//! Within that window the agent can exit and the OS can reissue its process
//! group id to something else entirely, at which point signalling the cached
//! number is the very failure `crate::dispatch::DispatchSidecar`'s
//! authentication exists to prevent, merely with a shorter fuse.
//!
//! So the kill path **re-authenticates on the thread that signals**, at the
//! last possible moment, by re-running
//! [`crate::dispatch_inspect::probe_liveness`] — the same function the worker
//! uses, deliberately not a lighter-weight check written next to the kill.
//! One authenticated path, one set of identity rules. A run that no longer
//! authenticates is reported, not signalled.
//!
//! The residual window is now the microseconds between the probe returning
//! and `kill` being called, against a `ps` scan that has just confirmed this
//! run's own prompt path is on a live process in that group.
//!
//! ## Why this is its own module
//!
//! It signals, so it cannot live in `dispatch_inspect` (read-only, by
//! contract). It needs that module's verdict, so putting it in `dispatch`
//! would point the pipeline module at its own inspector. A third small module
//! that depends on both keeps each one's job describable in a sentence.

use crate::dispatch_inspect::{probe_liveness, DispatchRunLiveness, SidecarDoubt};
use crate::kill::{kill_pgid, KillOutcome};
use std::time::Duration;

/// Why a kill was not attempted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KillRefusal {
    /// The run's process group is verifiably gone. The overwhelmingly likely
    /// reason a re-authentication fails: the agent finished between the
    /// worker's last probe and the click.
    AlreadyEnded,
    /// The sidecar stopped being trustworthy — it was rewritten, or the
    /// process table could not be read this time. Distinct from
    /// [`Self::AlreadyEnded`] because "we can't tell" is not "it's over".
    NotVerifiable(SidecarDoubt),
}

/// What happened when a user asked to stop a run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DispatchKillOutcome {
    /// The signal went to a group re-authenticated moments earlier.
    /// `supervised` is read from the *fresh* probe too, so the report of what
    /// happens next can't be stale either.
    Signalled {
        pgid: i32,
        supervised: bool,
        outcome: KillOutcome,
    },
    /// The group authenticated but the signal itself errored.
    SignalFailed { pgid: i32, error: String },
    /// Nothing was signalled.
    Refused(KillRefusal),
}

impl DispatchKillOutcome {
    /// Whether a signal actually left the process. The property worth
    /// asserting in a test: a refusal must be inert.
    pub fn signalled(&self) -> bool {
        matches!(
            self,
            DispatchKillOutcome::Signalled { .. } | DispatchKillOutcome::SignalFailed { .. }
        )
    }

    /// One line for a status surface. Says what happened *and* what it means
    /// for the task, because "killed" alone leaves a user guessing whether
    /// any bookkeeping followed.
    pub fn describe(&self, task_id: &str) -> String {
        match self {
            DispatchKillOutcome::Signalled {
                pgid,
                supervised,
                outcome,
            } => {
                let verb = match outcome {
                    KillOutcome::Terminated => format!("killed pgid {pgid} (SIGTERM)"),
                    KillOutcome::Killed => format!("killed pgid {pgid} (SIGKILL)"),
                    // Re-authenticated microseconds earlier, so this is a
                    // genuine race rather than the stale-handle case above.
                    KillOutcome::NotFound => format!("pgid {pgid} exited as it was signalled"),
                };
                let tail = if *supervised {
                    ""
                } else {
                    " — unsupervised, so the task stays on `dispatching`"
                };
                format!("{task_id}: {verb}{tail}")
            }
            DispatchKillOutcome::SignalFailed { pgid, error } => {
                format!("{task_id}: kill {pgid} failed: {error}")
            }
            DispatchKillOutcome::Refused(KillRefusal::AlreadyEnded) => {
                format!("{task_id}: run already ended — nothing killed")
            }
            DispatchKillOutcome::Refused(KillRefusal::NotVerifiable(doubt)) => {
                format!("{task_id}: nothing killed — {}", doubt.explain())
            }
        }
    }
}

/// Re-authenticate `task_id`'s run and signal it only if it is still the run
/// that was recorded. See the module doc for why the re-check is not optional.
///
/// Blocking (it runs a `ps` and then waits out `grace` on the signal), so
/// callers hand it a thread — `HiveApp::spawn_kill_dispatch` does.
pub fn kill_dispatch_run(
    task_id: &str,
    started_at_unix: u64,
    grace: Duration,
) -> DispatchKillOutcome {
    match probe_liveness(task_id, started_at_unix) {
        DispatchRunLiveness::Alive { pgid, supervised } => match kill_pgid(pgid, grace) {
            Ok(outcome) => DispatchKillOutcome::Signalled {
                pgid,
                supervised,
                outcome,
            },
            Err(e) => DispatchKillOutcome::SignalFailed {
                pgid,
                error: e.to_string(),
            },
        },
        // `NoSidecar` and `Gone` are the same news to a user who just clicked
        // Kill: the run they were looking at is over. The pipeline releasing
        // it (which removes the sidecar) and the agent dying are both this.
        DispatchRunLiveness::NoSidecar | DispatchRunLiveness::Gone => {
            DispatchKillOutcome::Refused(KillRefusal::AlreadyEnded)
        }
        DispatchRunLiveness::Unverifiable(doubt) => {
            DispatchKillOutcome::Refused(KillRefusal::NotVerifiable(doubt))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_refusal_is_inert_and_a_signal_is_not() {
        assert!(!DispatchKillOutcome::Refused(KillRefusal::AlreadyEnded).signalled());
        assert!(
            !DispatchKillOutcome::Refused(KillRefusal::NotVerifiable(SidecarDoubt::StaleBoot))
                .signalled()
        );
        assert!(DispatchKillOutcome::Signalled {
            pgid: 1,
            supervised: true,
            outcome: KillOutcome::Terminated,
        }
        .signalled());
        assert!(DispatchKillOutcome::SignalFailed {
            pgid: 1,
            error: "boom".to_string(),
        }
        .signalled());
    }

    /// The status line a user sees is the whole point of refusing rather than
    /// silently doing nothing.
    #[test]
    fn a_refused_kill_says_nothing_was_killed() {
        let text = DispatchKillOutcome::Refused(KillRefusal::AlreadyEnded).describe("TASK-1");

        assert!(text.contains("already ended"), "{text}");
        assert!(text.contains("nothing killed"), "{text}");
    }

    #[test]
    fn an_unverifiable_refusal_carries_its_reason() {
        let text =
            DispatchKillOutcome::Refused(KillRefusal::NotVerifiable(SidecarDoubt::StaleBoot))
                .describe("TASK-1");

        assert!(text.contains("nothing killed"), "{text}");
        assert!(text.contains("before the last reboot"), "{text}");
    }

    /// A supervised kill may promise the release; an unsupervised one must
    /// not. Same distinction the confirm dialog draws, held at the end of the
    /// operation too.
    #[test]
    fn only_an_unsupervised_kill_warns_the_task_is_left_claimed() {
        let supervised = DispatchKillOutcome::Signalled {
            pgid: 42,
            supervised: true,
            outcome: KillOutcome::Terminated,
        }
        .describe("TASK-1");
        let orphaned = DispatchKillOutcome::Signalled {
            pgid: 42,
            supervised: false,
            outcome: KillOutcome::Terminated,
        }
        .describe("TASK-1");

        assert!(!supervised.contains("dispatching"), "{supervised}");
        assert!(orphaned.contains("stays on `dispatching`"), "{orphaned}");
    }

    /// A group that exits in the microseconds between the probe and the
    /// signal is a race, not a stale handle — the copy must not accuse the
    /// user of clicking a dead button.
    #[test]
    fn a_group_that_exits_mid_signal_reads_as_a_race() {
        let text = DispatchKillOutcome::Signalled {
            pgid: 42,
            supervised: true,
            outcome: KillOutcome::NotFound,
        }
        .describe("TASK-1");

        assert!(text.contains("exited as it was signalled"), "{text}");
    }
}
