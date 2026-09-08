# PR refresh countdown (TASK-168)

The first PR observation row shows the remaining refresh interval, then `refreshing` while a request is pending. Completion starts a fresh configured interval. Remove the separate refreshing note while keeping errors, enrichment warnings, coverage and stale-data health visible. The interval is already configurable through `pr_refresh_seconds`; do not add another clock or setting.

The isolated worktree started clean at `be9428d`. The primary checkout's existing ranking and TASK-150 edits are outside this change. TASK-168 was filed and claimed before implementation.

## State and stress matrix

| State or dimension | Required evidence |
| --- | --- |
| Initial load, no snapshot | First-row refreshing indicator; loading body remains honest. |
| Idle countdown | Render full interval, intermediate seconds and zero from the scheduling clock. |
| Slow refresh with cached rows | Countdown becomes refreshing and cached content stays visible; no duplicate refreshing note. |
| Success, empty or populated results | Completion restarts the full configured interval. Existing empty/list behavior remains compatible. |
| Failure, offline or access denied | Error remains visible; countdown restarts for a bounded retry. Cached rows stay stale. |
| Manual retry, repeated refresh | Existing refresh key starts work immediately; one worker maximum. |
| Navigation and sustained key input | Hidden page does not initiate periodic requests; completed work is collected; user input cannot starve the refresh clock. |
| Narrow/current/wide and short terminals | Real Ratatui container renders without panic; first-row status remains available at supported widths. |
| Long content, partial results, warnings | Existing clipping/wrapping and warning behavior remains; no new content-dependent loop. |
| Reload/configuration changes | Existing interval configuration controls both countdown and scheduling. |
| Dirty/saving/conflict and role transitions | N/A: this surface reads observations and does not save or mutate GitHub state. Permission errors use the failure state. |
| Pointer/touch/browser zoom | N/A: keyboard-driven terminal UI; terminal cell dimensions cover layout stress. |
| Languages, duplicates and mixed row types | Unchanged PR data rendering; no new parsing or row identity behavior. Existing suite is regression evidence. |

## Evidence and remaining gates

The targeted command `CARGO_TARGET_DIR=/tmp/switchbard-pr-countdown-target SBT_PR_REPO=/Users/bpc/Dev/switchbard cargo test -p switchbard-tui --test pr_refresh --test pull_requests -- --include-ignored` passed all eight tests, including live read-only GitHub observations. `tests/pr_refresh.rs` exercises real workers and rendered countdown, visible zero, initial loading, delayed completion, manual retry, failure retry, cached success/failure and hidden-page scheduling at 40x8, 60x16, 100x24 and 180x40. Compact output is retained in [terminal evidence](tui-pr-refresh-terminal-evidence.md).

Independent read-only review found and verified corrections for a skipped visible zero and the restored-page initial frame. The timer rounds remaining duration down to display zero during the final fractional second; requests still begin at the deadline. The event loop ticks before its first frame and at a bounded cadence during continuous keyboard input. One completion clock in `pull_requests.rs` governs both the displayed countdown and automatic scheduling; the renderer does not invent a separate deadline.

Remaining scoped gaps: thread-creation failure and worker disconnection are handled but not induced by the E2E tests; continuous-key scheduling and initial process rendering have code-review evidence, not an automated PTY timing assertion. Human visual approval remains separate from automated terminal-render evidence. This task does not require GitHub writes or an egui performance smoke because only the TUI is changed.


The canonical `mise run tui-install` passed formatting, warning-free Clippy, and the complete default TUI test suite, then replaced `/Users/bpc/.cargo/bin/sbt` with the release build from this worktree. Live tests excluded by the default suite were run in the targeted command above. The change is locally committed and installed; no push, PR or merge is claimed.
