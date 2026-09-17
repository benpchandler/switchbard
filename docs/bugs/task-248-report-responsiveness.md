# TASK-248: responsive idea and bug submission

## Outcome

`:idea` and `:bug` capture their context and destination, then save through the core write layer on a single background worker. A persistent banner shows saving, success, or failure across pages. Pending duplicate submissions retain their text instead of creating another task; failures retain a retry draft. Ordinary quit and binary reexec wait for the pending result.

Periodic task refreshes also run off-thread. Stale results cannot replace newer local mutations. Focused detail refreshes read the backlog and authoritative edit snapshot under one repository fence in the worker; completion performs no storage reads. Entering or applying an edit defers while the report or refresh is pending. Read-only pane navigation remains available. Selection follows the filed task only when the original context remains current.

TASK-248 is the canonical tracker: high priority, Bugs project, labels tui/bug/performance. This change is isolated on `fix/idea-report-responsive`, based on the reconciled `origin/main` at `5e699a5c1b4bd4fe892e5a28c84f27dc3702be25`. Both the primary checkout and new worktree were initially clean. No PR, merge, or installation was performed in this implementation slice; full workspace CI remains a before-merge gate.

## Reproduction and validation

The installed build's live event log recorded `command idea` at 2026-09-16 11:20:49 taking 51847.37975 ms (TASK-246), and at 11:22:54 taking 33612.574584 ms (TASK-247). Both persisted successfully. The exact historical storage bottleneck remains unprofiled; the proven defect was performing storage work on the input thread. Numeric ID matching was valid and was not changed.

Two real-key regressions failed before their fixes: Enter blocked for 3.211167041 seconds under a two-second repository lock; focused-detail refresh adoption blocked for 2.320407417 seconds under contention. Both now pass a 500 ms input-thread bound. Native PTY validation rendered Saving in 0.056 seconds under a real lock, navigated to Inbox, deferred quit and binary replacement, and verified exactly one persisted report before automatic reexec.

- TUI coverage across reconciled initial, continuation and focused runs: 441 passed, 0 remaining failures, 22 existing opt-in ignored; every integration binary covered. An earlier compiled polling-helper version failed a PR-loading timing assertion; the unrelated helper change was removed, and rebuilt navigation tests passed. Final focused reruns covered the last production changes.
- Final TUI all-target clippy, formatting, doc tests and Git whitespace checks passed. No doc tests are defined.
- Repository developer gates passed, including hook, CI-routing and installer contracts.
- Existing native terminal EOF and SIGTERM regression proofs passed on the final debug binary.
- Independent adversarial review found no remaining production blocker. It verified context capture, stale-generation handling, pending lifecycle, and the absence of storage I/O in asynchronous completion.

Evidence logs: `/tmp/task248-baseline.txt`, `/tmp/task248-detail-baseline.log`, `/tmp/task248-suite-summary.json`, `/tmp/task248-tui-tests.log`, `/tmp/task248-remaining.log`, `/tmp/task248-final-validation.log`, `/tmp/task248-final-clippy.log`, `/tmp/task248-final-doc.log`, `/tmp/task248-native-final.log`, `/tmp/task248-tty-final.log`, and `/tmp/switchbard-task248-dev-gates.log`. Native probe: `/tmp/task248-native.py`.

## State and stress coverage

| State or transition | Evidence |
| --- | --- |
| Idle / empty intent | `tests/browse.rs`: rejects empty report without writing |
| Saving / real storage latency | `tests/report.rs`: bounded Enter and tick, rendered saving banner; native PTY proof |
| Success / duplicate submissions | Exactly one real task, retained pending second draft, kind metadata and context preserved |
| Configured other repository | `tests/browse.rs`: correct destination, current list preserved |
| Failure / retry | Real filesystem failure writes no task; Unicode draft restored and retry succeeds |
| Navigation / result persistence | Page changes retain confirmation; selection is not stolen after navigation |
| Quit / automatic binary replacement | Key E2E plus native PTY: pending operation defers both until persistence |
| Stale / unavailable / recovered storage | Central-storage and detail-edit journeys; generation and exact snapshot guards reviewed |
| Focused detail completion | Real-lock tick regression, visible refreshed row at 100/160 columns, subsequent Done edit succeeds |
| Filtered new task / key release and repeat | Full-ID confirmation even when hidden; ignored repeat cannot submit another report |
| Narrow / short / normal / wide | 40x8 report confirmation, 100/160-column detail screens, existing list and Inbox size matrices |
| Zero / one / many tasks | Existing task-list and 250-task stress journeys retained |
| Unknown outcome / worker loss | Explicit unknown-outcome message, no automatic retry; reviewed, not forcibly injected |
| Read-only / permission-limited | Existing editability guard retained; report-specific permission denial not separately injected |
| Historical / role changes / network offline | N/A to local report creation; no role or remote API lifecycle added |
| Pointer / touch / zoom | Existing detail pointer tests pass; touch/browser zoom N/A to terminal command flow |

## Limits

The change does not add durable crash recovery for forced process termination, terminal loss, or machine shutdown. Existing bounded terminal-hangup exit behavior remains intact. It removes blocking work from report submission and automatic refresh, without claiming to reduce the underlying storage operation's duration or make every unrelated mutation asynchronous.

Thread-spawn failure and worker-channel disconnect are handled and reviewed but not forcibly injected. Report-specific maximum-length multiline content is not separately exercised. There is no owner visual-approval claim. Installed `sbt` was still `5a9e9ae` at closeout; this fix is a validated local commit awaiting delivery.
