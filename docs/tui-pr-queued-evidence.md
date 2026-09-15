# Pull Requests merge queue status

## Contract

An authoritative GitHub `mergeQueueEntry` makes an open PR read Queued. Lifecycle stays Open; `status:open` includes Queued, and `status:queued` selects only observed queue entries. Checks, approval and auto-merge settings cannot imply queue membership. Closed and merged lifecycle wins over stale queue observations. The existing `lifecycle:` alias retains the same filter behavior as `status:`.

One optional read requests at most 100 loaded open PR IDs. A result joins only on the same opaque ID, number, URL, head revision and open lifecycle. Missing, changed or unavailable observations stay Unknown with a separate queue warning. Queue failures do not suppress valid check-change notifications. Queue entry/exit notifications require two known observations, preventing failed reads from announcing a false exit. Simultaneous delivery and queue warnings have a combined list summary and complete causes in the scrollable detail pane.

## State and stress matrix

| States and stresses | Evidence |
|---|---|
| Queued, ordinary Open, Closed, Merged; queue entry/exit | `pr_list::queue::tests::queue_membership_refines_status_but_never_lifecycle_checks_or_merged_state`; live row/filter/detail journey |
| Unknown, partial coverage, identity/head/lifecycle changed, null/missing fields, permission/error response, oversize response | Four `pr_list::queue::tests`; 101 loaded rows prove explicit partial coverage; independent cached-state warning render |
| Known queue entry/exit alerts, repeated observations, Unknown transitions, cross-page dismissal | `known_queue_changes_notify_across_pages_unknown_does_not_claim_exit` uses cached observations and real keys/rendering |
| Refresh/loading, stale/retry, page switch, restart | Existing PR refresh/controls E2E plus live queue journey: repository remote removed locally to cause a real failed refresh; retained row marked STALE |
| Keyboard filter picker, semantic sort, categorical paint, Open filter compatibility | Live queue journey selects Queued and paints it through shared controls; existing task settings remain independent |
| 80x24, 120x40, 180x50, 40x12 | Live queue journey renders list/detail at each size. At 40x12 split width, list cells truncate as before; detail metadata still reads Queued |
| Zero/many/1000-row metadata, long titles/mixed scripts | Existing bounded list/presentation gates are unchanged; queue-specific 101-row partial-coverage test. Real #985 title wraps across narrow detail lines |
| Dirty/saving/conflict/role mutation/touch/browser zoom | N/A: read-only terminal observation; existing merge controls unchanged |
| Live queue-to-merged transition | No GitHub writes authorized; deterministic core lifecycle precedence and cached alert coverage only |

## Baseline and live proof

On 2026-09-15, the real authenticated TUI harness `SBT_PR_REPO=/Users/bpc/Dev/budget cargo test -p switchbard-tui --test pull_requests live_repository_renders_actual_pull_requests -- --ignored --nocapture` rendered #985 as Open / Passed. An independent GitHub `mergeQueueEntry` read reported position 1, AWAITING_CHECKS. Queue status was missing even with passing head checks.

The same live harness after the change rendered #985 as Queued / Passed, with `Merge queue: Queued` in details. `SBT_PR_REPO=/Users/bpc/Dev/budget SBT_QUEUED_PR=985 cargo test -p switchbard-tui --test pr_queued -- --ignored --nocapture` passed the real queue picker/filter/sort/paint/resize/restart/stale-refresh journey. This opt-in test requires an actually queued PR and does not mutate GitHub.

The two non-network `pr_queued` journeys inject cached observation state and exercise real keys/rendering. They prove combined-warning layout and notification semantics; they do not claim reproduction of simultaneous remote failures or a live enqueue/dequeue event.

## Verification and gaps

Formatting and core/TUI clippy passed. Four core queue tests and two cached-state TUI journeys passed. The full core/TUI suite passed 1097 tests with 25 opt-in tests ignored; the final added cases then passed in the focused core and TUI commands. Durable local evidence lives at `/Users/bpc/.codex/artifacts/switchbard-pr-queued-20260915/`; logs include the real terminal renders and test results.

Queue order, queue position, merge-group check progress, enqueue/dequeue actions and auto-merge configuration are outside this display change. Native owner visual approval, installed build, CI, PR and merge remain separate gates and are not implied by local tests.
