//! User-visible "last action" feedback strings, shared across threads.
//!
//! Each view has its own `Status` (config / servers / listeners) — the
//! domains are distinct, but they all want the same contract: a worker
//! thread writes a one-line outcome, the UI reads and displays it. This
//! type just hides the `Arc<Mutex<Option<String>>>` plumbing.

use std::sync::{Arc, Mutex};

#[derive(Clone, Default)]
pub struct Status(Arc<Mutex<Option<String>>>);

impl Status {
    pub fn new() -> Self {
        Self::default()
    }

    /// Replace the message with `msg`. Use `format!` at the call site —
    /// callers want context-specific phrasing.
    pub fn set(&self, msg: impl Into<String>) {
        *self.0.lock().unwrap() = Some(msg.into());
    }

    /// Snapshot the current message without clearing it. The UI calls this
    /// each frame; the message persists until something replaces it.
    pub fn snapshot(&self) -> Option<String> {
        self.0.lock().unwrap().clone()
    }
}
