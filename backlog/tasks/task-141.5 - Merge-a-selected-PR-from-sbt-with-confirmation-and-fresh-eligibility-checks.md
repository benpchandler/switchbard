---
id: TASK-141.5
title: Merge a selected PR from sbt with confirmation and fresh eligibility checks
status: In Progress
assignee: []
created_date: '2026-09-08 11:10'
updated_date: '2026-09-08 11:18'
labels:
  - tui
  - github
  - merge
  - ball:agent
dependencies: []
priority: high
parent_task_id: '141'
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Impact: sbt PR users must leave the app to merge; an unguarded shortcut could merge the wrong revision. Evidence: owner request 2026-09-08 to add an in-app merge option; eec92a0 has browser opening and observations but no merge action. This is the direct-merge TUI slice of TASK-119; auto-merge, queue actions, branch update and CI rerun remain separate. Preserve Task 141 browser, alerts and countdown. Use exact-head confirmation, fresh core eligibility, bounded off-thread execution, durable receipt and readback. No admin override or branch deletion.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The configurable m action shows current eligibility and exact PR identity, head, base, viewer and supported merge methods before explicit confirmation; Cancel is the default.
- [ ] #2 The core revalidates identity, head, permissions and GitHub readiness at execution, uses an expected-head server guard, and never offers admin override, queue bypass or branch deletion.
- [ ] #3 Execution remains responsive and single-flight across page switches; failures and ambiguous outcomes are explicit, with durable receipts and GitHub readback; task completion is untouched.
- [ ] #4 Core guard/write/readback tests and real-key TUI read-only journeys pass; the installed sbt preserves prior PR controls, alerts and countdown.
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implementation contract and state/stress matrix: docs/tui-pr-merge-ledger.md. User requested direct in-app merge; prior Task 141 commits retained. Existing parked delivery branch untouched.

Implemented and installed. Full TUI fmt/clippy/tests/install passed; real GitHub historical-rejection and preparation-cancellation journeys passed. Core receipt/guard/readback tests passed before final integration. Full workspace preflight and independent adversarial review running; positive eligible preview/cancel follows on delivery PR. No actual remote PR merged.
<!-- SECTION:NOTES:END -->
