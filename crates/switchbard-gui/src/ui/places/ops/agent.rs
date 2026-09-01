//! TASK-100 Agent cell provider — and the seam TASK-98 wires into.
//!
//! The mock's Agent column shows two things: which agent is attributed to a
//! worktree, and roughly how long it's been active there ("claude · active
//! 1h"). Today the only source of that fact is a dispatch run — `DispatchRun`
//! already knows its `worktree_path` and `started_at_unix`, and headless
//! dispatch only ever runs `claude -p`, so "claude" is not a guess.
//!
//! **Named gap:** an *interactive* agent session — someone with a live
//! `claude` (or other agent) terminal open in a worktree, not a headless
//! dispatch run — is invisible to this cell today. TASK-98's `agent_sessions`
//! core capability is what would make that visible; it was not on `main`
//! when this task landed (confirmed via `git log --all`). This module is the
//! seam: `Snapshot::agent_attribution_by_wt` is built from dispatch-run data
//! alone (`accumulate`), and `label` renders it. Once `agent_sessions`
//! exists, its provider slots in beside this one (dispatch-run attribution ∪
//! interactive-session attribution) — `ui::places::ops::row::render_agent_
//! cell` calls only `label` today, so wiring the union in later touches this
//! file and one call site, nowhere else in the row-rendering path.

use std::collections::HashMap;
use std::path::PathBuf;

use switchbard_core::dispatch_inspect::DispatchRun;
use switchbard_core::humanize_age;

/// What `Snapshot::collect` accumulates per worktree from the dispatch run
/// table: how many live runs hold it, and the oldest `started_at_unix` among
/// them (the longest continuously-active one, which is the more useful
/// "active since" figure than the newest).
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct DispatchAttribution {
    pub(crate) count: usize,
    pub(crate) oldest_started_at_unix: Option<u64>,
}

/// Fold one worktree-holding `DispatchRun` into the accumulator for its
/// worktree. Called once per holding run in `Snapshot::collect`.
pub(crate) fn accumulate(by_wt: &mut HashMap<PathBuf, DispatchAttribution>, run: &DispatchRun) {
    let entry = by_wt.entry(run.worktree_path.clone()).or_default();
    entry.count += 1;
    entry.oldest_started_at_unix = match (entry.oldest_started_at_unix, run.started_at_unix) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (Some(a), None) => Some(a),
        (None, b) => b,
    };
}

/// One line for the Agent cell: "claude · active 1h", or just "claude" if
/// the run's start time couldn't be recovered from its log filename. A
/// worktree held by more than one live run (rare — usually one dispatch run
/// per worktree at a time) appends a count so the cell doesn't quietly
/// under-report how many are actually in flight there.
pub(crate) fn label(attribution: &DispatchAttribution) -> String {
    let base = match attribution.oldest_started_at_unix {
        // `humanize_age` always suffixes " ago" (right for a commit/activity
        // timestamp, which is what every other caller uses it for) — stripped
        // here because "active 1h ago" reads as a contradiction (present tense
        // "active" against past-tense "ago"); the mock's own wording is
        // "active 1h".
        Some(unix) => format!(
            "claude · active {}",
            humanize_age(unix).trim_end_matches(" ago")
        ),
        None => "claude".to_string(),
    };
    if attribution.count > 1 {
        format!("{base} (×{})", attribution.count)
    } else {
        base
    }
}
