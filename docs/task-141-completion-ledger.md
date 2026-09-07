# TASK-141 completion ledger

Outcome: finish the PR page with actions and status notifications across pages, preserving the merged column/filter/sort/paint/view parity from PR #136 (merged 2026-09-07; eight passing checks).

Initial implementation worktree: `/Users/bpc/Dev/.worktrees/switchbard-finish-141`, branch `feat/finish-task-141`, clean at e78b51e. Primary checkout has unrelated ranking.yml and TASK-150 dirt and is read-only. Task writes use sb in the isolated worktree. Claim: codex-finish-141, PID 61526.

Action scope: open the selected PR in its browser with O; existing refresh r, detail Enter and more remain. Guarded review/merge mutations have separate backend contracts (TASK-118 and adjacent GitHub Operations tasks); owner was asked asynchronously whether to expand this task. Default is browser actions plus notifications. No external PR mutations are authorized by implementation testing.

Notifications: after the first PR visit, continue bounded periodic reads on either page. First snapshot establishes a baseline. Observe changes to previously seen PR lifecycle/check/review/merge/draft fields and refresh availability. Retain at most 32 session alerts, deduplicate identical failure polls, show latest across pages, explicit n acknowledges one alert. Restart establishes a fresh baseline; this is a session status surface, not a durable activity history.

## Acceptance and evidence

- Reporter confirmation in original AC remains unchecked until the reporter explicitly confirms.
- PR browser action and empty selection error use shared key bindings; help and footer expose actions.
- Background observation and failure/recovery notifications survive page switching without altering task/view state.
- Source checks, E2E journeys, full preflight, installation, PR/CI and reporter confirmation are separate evidence gates.

## State and stress matrix

| State | Evidence / gate |
| --- | --- |
| First visit / loading / empty / no selection | Real-key E2E; no historic event flood; browser action reports no selection. |
| Active / historical PRs / unchanged refresh | Authenticated GitHub reads; initial baseline and unchanged observation checks. |
| Failure / retry / recovery / stale cached data / offline | Real temporary repo failures and real remote removal/re-add journeys. |
| Lifecycle / check / review / merge changes | Compare bounded observations; live mutation testing requires an authorized test PR, recorded gap otherwise. |
| Narrow / current / wide / short / Unicode | Real TestBackend cell renders; no fixed pixel assumptions. |
| Keyboard / repeated actions / page switch / editing | Alert is independent of transient status/input; dismissal explicit. |
| Scale / retention | 1000 fetched PR cap retained; 32 notification cap. |
| Restart | Session alerts reset; existing PR selection/view resume behavior retained. |
| Pointer / touch / zoom / save conflicts | N/A for keyboard terminal observations without a new persistent write store. |
| Reporter appearance confirmation | Explicit outstanding acceptance gate; automated tests cannot supply it. |

Command owns integration/app/config/tests/docs/tracker/delivery. Notification worker owns PullRequests and rendering. Scout independently audited remaining scope. No shared app or watcher restart.
