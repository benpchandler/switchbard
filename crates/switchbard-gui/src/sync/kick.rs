//! Tiny condvar-based "wake the worker" primitive.
//!
//! Worker threads sleep with `kick.wait(period)`; the UI calls `kick.notify()`
//! when it wants the worker to re-run immediately (e.g. the user added a repo
//! or edited a task). The wrapper just hides the `Mutex<bool> + Condvar` pair
//! so call sites don't repeat the same five-line dance.
//!
//! ## The kick is sticky, and has to be
//!
//! The obvious implementation — a bare `Mutex<()> + Condvar` whose `notify`
//! only calls `notify_all()` — **silently drops any kick raised while the
//! worker is mid-tick**, because `notify_all` wakes threads that are *already
//! parked* and leaves no record for one that isn't. That is not a rare race
//! here: `workers.rs`'s own cadence table measures ticks of ~6-8s (git probe)
//! and up to ~47s (a cold agent-context pass), so a click landing inside a
//! tick is routine. The lost kick then costs the user a full period —
//! 30s/60s/120s depending on the worker, doubled again by
//! `UNFOCUSED_BACKOFF_MULTIPLIER` — which reads as "my change didn't cascade
//! through the app at all" rather than as latency.
//!
//! So `notify` records a pending flag under the mutex *before* signalling, and
//! `wait` consumes it. A kick raised at any point before `wait` is observed by
//! that `wait`, whether or not anyone was parked when it fired. Consuming is
//! sound because each `Kick` has exactly one waiting worker — `Channels` hands
//! every worker its own (`scanner_kick`, `backlog_kick`, …), and cloning only
//! shares the handle with the UI side, which never waits.
//!
//! Using `wait_timeout_while` rather than a bare `wait_timeout` also makes the
//! flag the loop predicate, so a spurious wakeup resumes waiting instead of
//! being mistaken for a real kick.

use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

#[derive(Clone, Default)]
pub struct Kick(Arc<(Mutex<bool>, Condvar)>);

impl Kick {
    pub fn new() -> Self {
        Self::default()
    }

    /// Request that the worker re-run as soon as it can. Safe to call when
    /// the worker is busy rather than parked — see the module doc; the kick
    /// is latched and honoured by its next `wait`.
    pub fn notify(&self) {
        let (lock, cvar) = &*self.0;
        {
            let mut pending = lock.lock().unwrap();
            *pending = true;
        }
        cvar.notify_all();
    }

    /// Block up to `dur`, returning early if a kick is pending or arrives.
    /// Consumes the pending kick so the following `wait` sleeps normally.
    pub fn wait(&self, dur: Duration) {
        let (lock, cvar) = &*self.0;
        let pending = lock.lock().unwrap();
        let (mut pending, _timeout) = cvar
            .wait_timeout_while(pending, dur, |pending| !*pending)
            .unwrap();
        *pending = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    /// The regression this type exists for: a kick raised while the worker is
    /// working (i.e. nobody parked) must still be observed by the next
    /// `wait`, not silently dropped into a full-period sleep.
    #[test]
    fn notify_before_wait_is_not_lost() {
        let kick = Kick::new();
        kick.notify();

        let start = Instant::now();
        kick.wait(Duration::from_secs(30));
        assert!(
            start.elapsed() < Duration::from_secs(1),
            "a kick raised before wait() must return immediately, not sleep the period"
        );
    }

    /// A kick is a one-shot: once consumed, the next wait sleeps normally
    /// rather than spinning the worker in a tight loop.
    #[test]
    fn kick_is_consumed_by_the_wait_that_observes_it() {
        let kick = Kick::new();
        kick.notify();
        kick.wait(Duration::from_secs(30));

        let start = Instant::now();
        kick.wait(Duration::from_millis(150));
        assert!(
            start.elapsed() >= Duration::from_millis(100),
            "the second wait should time out normally, not see a stale kick"
        );
    }

    /// With no kick at all, `wait` is just a sleep for the period.
    #[test]
    fn wait_times_out_without_a_kick() {
        let kick = Kick::new();
        let start = Instant::now();
        kick.wait(Duration::from_millis(150));
        assert!(start.elapsed() >= Duration::from_millis(100));
    }

    /// The originally-working case must keep working: a kick delivered while
    /// the worker *is* parked wakes it promptly.
    #[test]
    fn notify_wakes_a_parked_waiter() {
        let kick = Kick::new();
        let waiter = kick.clone();
        let handle = std::thread::spawn(move || {
            let start = Instant::now();
            waiter.wait(Duration::from_secs(30));
            start.elapsed()
        });

        std::thread::sleep(Duration::from_millis(100));
        kick.notify();

        let elapsed = handle.join().expect("waiter thread panicked");
        assert!(
            elapsed < Duration::from_secs(5),
            "a parked waiter must be woken by notify, took {elapsed:?}"
        );
    }
}
