//! Tiny condvar-based "wake the worker" primitive.
//!
//! Worker threads sleep with `kick.wait(period)`; the UI calls `kick.notify()`
//! when it wants the worker to re-run immediately (e.g. user clicked Refresh).
//! The wrapper just hides the `Mutex<()> + Condvar` pair so call sites don't
//! repeat the same five-line dance.

use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

#[derive(Clone, Default)]
pub struct Kick(Arc<(Mutex<()>, Condvar)>);

impl Kick {
    pub fn new() -> Self {
        Self::default()
    }

    /// Wake every thread parked on `wait`.
    pub fn notify(&self) {
        let (lock, cvar) = &*self.0;
        let _g = lock.lock().unwrap();
        cvar.notify_all();
    }

    /// Block up to `dur`, returning when either the timeout elapses or
    /// `notify()` is called from another thread.
    pub fn wait(&self, dur: Duration) {
        let (lock, cvar) = &*self.0;
        let guard = lock.lock().unwrap();
        let _ = cvar.wait_timeout(guard, dur).unwrap();
    }
}
