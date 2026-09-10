# PR controls parity objective ledger

## Outcome and authority

The owner requested Tasks UI parity for the PR pane, then identified filters, sorts, paint, and column controls as missing. The owner directed: queue these as tasks first, claim and work on them, and watch them through the system. The original detail-pane parity remains accepted implementation context, not a substitute for this larger outcome.

Authorized: create scoped backlog tasks through sb, live-claim them, implement, test, install the TUI, and drive delivery validation/PR/CI. Human visual approval and merge remain distinct final states. No PR review/merge mutations in the product, new headless dispatch run, or broader TASK-141 actions/notifications work is included.

## Work and dependencies

- TASK-141.1: shared PR columns/numbered actions and independent saved settings. Claimed first, session codex-pr-controls-20260907, live Codex PID 42373. The current sb protocol labels interactive claim agents claude; this is its fixed label, not a claim that a Claude process owns this work.
- TASK-141.2: filter parity, depends on 141.1.
- TASK-141.3: sort parity, depends on 141.1.
- TASK-141.4: paint parity, depends on 141.1 and 141.2.

Canonical tracker is /Users/bpc/Dev/switchbard, where the user's sbt is running. Only the four new task files are agent-owned there. Baseline dirt: backlog/ranking.yml and task-150 markdown; never stage, reset or rewrite them. Task creation and claim commits are cherry-picked into the clean implementation worktree /Users/bpc/Dev/.worktrees/switchbard-tui-pr-pane, branch feat/tui-pr-pane. This worktree was clean before this turn.

Command owns tests, tracker, ledger, integration, installation and delivery. Implementation unit owns TUI src and its CLAUDE.md; scout is read-only. No parallel Cargo gates or live-process restarts. Installed binary updates use the existing authorized TUI reload mechanism.

## Acceptance truth

Each task's three checkable criteria are authoritative; do not check them merely because implementation compiles. Prove real key events and rendered cells over real disk tasks and actual GitHub read observations. Preserve task settings, view compatibility, PR cached read safety and selected identity. Report implementation, commit, install, PR, CI, merge and user approval separately.

## State and stress matrix

| Dimension | Evidence plan |
| --- | --- |
| Default/active selection, no rows, no match, loading, unavailable, stale | Real process PR journeys, actual non-repo failure, retry and retained observations. |
| One/many rows, numeric IDs, semantic statuses/checks, unknown/historical values | Live GitHub snapshots; key-driven filters and ascending/descending sorts, compare resulting visible rows. |
| One/many linked task values, long/unbroken content | Create reference-bearing tasks through core in temporary repo linked to actual GitHub remote; assert filter and paint against rendered cells. |
| Narrow/current/wide/short | TestBackend renders at 40x12, 80x24, 120x40 and 180x50; shared detail pane regression retained. |
| Keyboard, repeated input, page switch, restart and saved slots | Shared picker keys, real saved files, App reopen/resume; verify Tasks state unchanged. |
| Filter/sort/paint combinations and selection | Behavioral sort/filter identities and rendered cell styles; refresh cached read and preserve selected PR where present. |
| Dirty/saving/conflict/outcome unknown/roles | No GitHub mutation added. View writes use existing local saved-view mechanism; save errors must remain explicit. |
| Pointer/touch/browser zoom | N/A: keyboard terminal UI, cell dimensions drive layout. |
| Unicode/extreme 1000-row scale/prolonged latency | Explicit gap unless exercised by real fixtures; preserve bounded fetch/paint/render paths. |
| Human appearance approval | User observes installed TUI; automated evidence does not imply approval. |

## Evidence and remaining gates

Tasks created and committed first (5e4fb5a primary / a0b347e implementation); first live claim committed (ff7402e primary / 67bc28b implementation). sb work list confirms TASK-141.1 with live PID. Implementation, E2E checks, install, no-mistakes delivery, PR and CI pending.

## Progress evidence

All four tasks are now live-claimed (primary commit 7bc9e50, implementation 60a01a7). The implementation uses the shared ViewState/picker contracts with separately retained page state and separate .prs.lua saved files. A read-only review is auditing page swaps and reloads independently.

Initial real-key tests failed as expected: PR digit 1 was unbound; s reported task-only controls. After the shared column implementation, pr_column_menu_and_saved_layout_are_isolated_from_tasks passes. The actual canonical task is visible in a TestBackend render of the real App: TASK-141.1, In Progress, working:1, session codex-pr PID 42373 in the detail pane. This proves the live work store reaches the TUI renderer, not human visual approval.

Final targeted pass: all 20 page/PR/control journeys passed with authenticated GitHub reads and the live task claim. Command: `SBT_PR_REPO=/Users/bpc/Dev/switchbard SBT_WORK_TASK=TASK-141.1 cargo test -p switchbard-tui --test pr_controls --test pull_requests --test pages -- --include-ignored`. Captured output: docs/tui-pr-controls-evidence.md. New tests cover repo/global saved layout isolation, header reorder, background task reload, numeric sorting with retained selection, facets, auto/row/column paint, rule ordering/deletion, independent saved paint/filter/sort, restart PR identity, and actual live work projection.

Review found a restart selection bug and it was fixed with a pending stable PR identity, resolved after fresh data arrives. Prior status vocabulary order and compact PR metadata widths were retained. Existing task paint first-value semantics are preserved through its adapter. Full TUI gate, install and delivery remain pending.

The final title/check sort and review/merge facet journey also passed against the budget repository's actual PR observations (read-only). Independent reviewer reports no remaining verified source blocker. `mise run tui-install` passed formatting, clippy, and the full TUI suite, then installed the release build. Source commit ea277a1; subsequent changes only extend tests/evidence. TASK-141.1 through .4 now have local implementation acceptance proved and move to In Review for delivery validation. Claims remain active while command follows the pipeline. Human appearance approval, PR checks and merge are still separate gates.
