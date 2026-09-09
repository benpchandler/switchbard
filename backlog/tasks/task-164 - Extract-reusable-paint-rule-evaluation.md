---
id: TASK-164
title: Extract reusable paint rule evaluation
status: In Review
assignee: []
created_date: '2026-09-07 10:05'
updated_date: '2026-09-08 18:12'
labels:
  - abstraction
  - refactor
  - paint
  - ball:me
dependencies:
  - TASK-162
  - TASK-163
priority: medium
project: Abstraction
references:
  - https://github.com/benpchandler/switchbard/pull/145
  - https://github.com/benpchandler/switchbard/pull/144
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Impact: developers extending list styling must couple new features to task fields and Ratatui colors or duplicate precedence logic, risking inconsistent highlights for users. Priority is medium because the gap affects reuse and future consistency.

Evidence: crates/switchbard-tui/src/paint.rs:8 imports BacklogTask, rule evaluation at :137 and field_value at :171 bind rules to tasks; :179 also accepts BacklogTask. Verified with rg -n "BacklogTask|field_value" on paint.rs. TASK-141.4 owns PR paint parity, and TASK-152 owns palette-token persistence; coordinate rather than duplicate those changes.

Discovery: owner-requested reuse assessment in session 01a07c1d-7259-73d1-82b7-97d68ba8ac87, recovered 2026-09-07; source verified at 7d09874 with rg/sed. Reconcile the implementation behind TASK-141.1 through TASK-141.4 before changing code; their PR parity behavior is not new scope here. Preserve existing commands and saved data. Choose between a small shared core contract and frontend-local adapters based on two concrete consumers; document the boundary and rationale before extraction, without a speculative registration framework.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Rule matching and precedence operate on the shared field/column contract for Tasks and an existing non-task list.
- [x] #2 Document and enforce the boundary between semantic paint decisions and frontend color/style conversion; do not introduce UI dependencies into core.
- [x] #3 Tests prove category, row and column rule precedence, multiple matches, no match, missing fields and invalid rule handling, preserving existing results.
- [x] #4 Current paint configuration remains compatible and palette-token behavior respects TASK-152; no second matcher is introduced.
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Owner-requested concrete consumer added 2026-09-07: TASK-167, painting by When merged and/or When task filed. Shape shared field access and paint evaluation to support these authoritative timestamps and composable time rules; coordinate with TASK-162/TASK-163. TASK-167 owns the visible feature and its date semantics, while this task retains the reusable evaluation boundary.

Implemented in feat/tui-abstractions: paint_eval.rs selects semantic tokens using shared field values and Filter, paint.rs retains terminal color parsing/conversion. Existing seven paint E2E journeys and new date/invalid-rule persistence journeys prove precedence and compatibility. docs/tui-date-paint-evidence.md records boundary and matrix. Root integration still in progress.

Delivery correction: local implementation and checks are complete on feat/tui-abstractions, but the agent still owes a PR. Keep visible as In Progress / ball:agent until a concrete review handoff exists. TASK-193 (Inbox and navigation badges) is the owner-prioritized first slice currently being delivered; it does not complete this abstraction task.

PR145 now exists, stacked on Inbox PR144. Agent still owns TASK199 correction and final validation before review handoff; keep In Progress / ball:agent and live delivery claim.

Released unfinished by session codex-ab: Review handoff: PR145 final head6c97bd50 passed6 CI checks with1 expected sidecar scope skip, then was externally merged into Inbox branch, mergeabd06558. Review combined PR144, which remains open against main; its new combined-head CI is tracked separately. Default sbt installation passed170 TUI tests/21 opt-in ignored; native navigation, orange count and linked task/PR checks passed. Human acceptance remains outstanding; TASK144 reporter criterion is not inferred.

Next action: owner reviews combined PR144 and confirms intended behavior; PR145 is merged into its branch, not main.

Latest delivery state: PR144 was externally merged into main at 2026-09-08T22:09:45Z, merge6d0fa5bf50ace83d27bf337a00fc277033761e73. PR145 and combined PR144 each passed6 CI checks plus1 expected scope skip. The combined abd06558 tree exactly matches6c97bd50; default installedd418 runtime differs only in documentation. Next action: owner verifies intended behavior in installed sbt and confirms acceptance; remain In Review / ball:me. TASK144 human reporter criterion remains unchecked. Main push CI is a separate check.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Implemented at code commit 815bcf01 on feat/tui-abstractions in /Users/bpc/Dev/.worktrees/switchbard-abstractions. Final isolated tui-install gate passed formatting, all-target clippy and 141 TUI tests; core suite passed 582 tests and core clippy. Seven opted-in live journeys passed, including authoritative GitHub date painting and a real live work claim. Installed and exercised /Users/bpc/.local/share/switchbard-builds/abstractions/bin/sbt in a real PTY. Existing default installation preserved because it contains an unmerged parent-picker feature. No push, PR or merge. Evidence: docs/tui-abstraction-mission.md and linked task-specific evidence documents.
<!-- SECTION:FINAL_SUMMARY:END -->
