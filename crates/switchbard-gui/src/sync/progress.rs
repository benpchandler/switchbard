//! Determinate progress for a long bulk action.
//!
//! [`Status`](super::Status) already carries a *message*, and every threaded
//! mutator sets one when it finishes. That is enough for an action that takes
//! a moment; it is not enough for one that makes a separate `backlog` CLI
//! call per task, where a 43-task sweep is many seconds of apparent nothing
//! and reads as a hang. This carries the countable half — done, total, and
//! what is being done — so the UI can draw a real bar rather than a spinner
//! that promises nothing.
//!
//! Deliberately a *separate* channel from `Status` rather than a richer
//! `Status`: every existing call site sets a plain message and should keep
//! compiling untouched, and the completion summary is still a message, not a
//! progress value. The two are set independently and the bar disappears while
//! the final summary stays.

use std::sync::{Arc, Mutex};

/// A bulk action's live progress. `None` means nothing is running.
#[derive(Clone, Default)]
pub struct Progress(Arc<Mutex<Option<BulkProgress>>>);

/// One in-flight bulk action.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BulkProgress {
    /// Items finished so far — successes *and* failures, since this measures
    /// how far through the batch we are, not how much of it worked. The
    /// success/failure split belongs in the completion summary.
    pub done: usize,
    /// Items in the batch. Never zero: a caller with nothing to do should not
    /// start, so a zero here would render a bar that can never fill.
    pub total: usize,
    /// Verb phrase for the label, e.g. `"archiving"`.
    pub verb: String,
}

impl BulkProgress {
    /// How full the bar should be, in `0.0..=1.0`.
    pub fn fraction(&self) -> f32 {
        if self.total == 0 {
            return 0.0;
        }
        (self.done as f32 / self.total as f32).clamp(0.0, 1.0)
    }

    /// `"archiving 12/43"` — the count is spelled out because a bar alone
    /// cannot say how much work is left in absolute terms, and "is this
    /// nearly done?" is the actual question being asked.
    pub fn label(&self) -> String {
        format!("{} {}/{}", self.verb, self.done, self.total)
    }
}

impl Progress {
    pub fn new() -> Self {
        Self::default()
    }

    /// Begin a run. Clears any previous one, so an abandoned progress value
    /// can never make a new action look partly finished before it starts.
    pub fn begin(&self, verb: impl Into<String>, total: usize) {
        *self.0.lock().unwrap() = Some(BulkProgress {
            done: 0,
            total,
            verb: verb.into(),
        });
    }

    /// Record one finished item. A no-op when nothing is running, so a stray
    /// call cannot conjure a bar out of nothing.
    pub fn advance(&self) {
        if let Some(progress) = self.0.lock().unwrap().as_mut() {
            progress.done = progress.done.saturating_add(1);
        }
    }

    /// End the run and hide the bar. Callers must do this on *every* exit
    /// path — a bar left at 42/43 is worse than no bar, because it claims a
    /// batch is still running when nothing is.
    pub fn finish(&self) {
        *self.0.lock().unwrap() = None;
    }

    /// Snapshot for rendering. `None` when idle.
    pub fn snapshot(&self) -> Option<BulkProgress> {
        self.0.lock().unwrap().clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fraction_spans_empty_to_full_and_cannot_exceed_it() {
        let p = Progress::new();
        p.begin("archiving", 4);
        assert_eq!(p.snapshot().unwrap().fraction(), 0.0);
        for _ in 0..4 {
            p.advance();
        }
        assert_eq!(p.snapshot().unwrap().fraction(), 1.0);
        // Over-advancing is clamped rather than overflowing the bar.
        p.advance();
        assert_eq!(p.snapshot().unwrap().fraction(), 1.0);
    }

    #[test]
    fn the_label_names_the_absolute_position_not_just_a_ratio() {
        let p = Progress::new();
        p.begin("archiving", 43);
        p.advance();
        assert_eq!(p.snapshot().unwrap().label(), "archiving 1/43");
    }

    #[test]
    fn finish_hides_the_bar_and_advance_alone_cannot_bring_it_back() {
        let p = Progress::new();
        p.begin("archiving", 2);
        p.finish();
        assert!(p.snapshot().is_none());
        p.advance();
        assert!(
            p.snapshot().is_none(),
            "a stray advance must not resurrect a finished run"
        );
    }

    /// A new run must not inherit the previous one's position.
    #[test]
    fn begin_resets_a_previous_run() {
        let p = Progress::new();
        p.begin("archiving", 10);
        p.advance();
        p.begin("completing", 3);
        let snapshot = p.snapshot().unwrap();
        assert_eq!(snapshot.done, 0);
        assert_eq!(snapshot.total, 3);
        assert_eq!(snapshot.verb, "completing");
    }
}
