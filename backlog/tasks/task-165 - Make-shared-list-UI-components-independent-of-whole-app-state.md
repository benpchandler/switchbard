---
id: TASK-165
title: Make shared list UI components independent of whole-app state
status: In Review
assignee: []
created_date: '2026-09-07 10:05'
updated_date: '2026-09-08 18:12'
labels:
  - abstraction
  - refactor
  - ui
  - tui
  - gui
  - ball:me
dependencies:
  - TASK-162
priority: medium
project: Abstraction
references:
  - https://github.com/benpchandler/switchbard/pull/145
  - https://github.com/benpchandler/switchbard/pull/144
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Impact: developers adding lists must wire rendering to the whole App or duplicate controls, increasing the chance that users encounter inconsistent keyboard, selection and layout behavior. Priority is medium because this is a maintainability and consistency gap.

Evidence: crates/switchbard-tui/src/view.rs:21 draw takes &mut App and draw_table also reads App; crates/switchbard-gui/src/ui/components/table_shell.rs:11 already provides a reusable GUI shell. Verified using sed -n "1,55p" on view.rs and rg -n "pub fn" on table_shell.rs. Build on the typed picker work in TASK-129 and module split in TASK-131; retain frontend-specific rendering.

Discovery: owner-requested reuse assessment in session 01a07c1d-7259-73d1-82b7-97d68ba8ac87, recovered 2026-09-07; source verified at 7d09874 with rg/sed. Reconcile the implementation behind TASK-141.1 through TASK-141.4 before changing code; their PR parity behavior is not new scope here. Preserve existing commands and saved data. Choose between a small shared core contract and frontend-local adapters based on two concrete consumers; document the boundary and rationale before extraction, without a speculative registration framework.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Extract bounded list/picker presentation inputs and explicit interaction outputs so reusable components do not require the entire App.
- [x] #2 Tasks and one existing non-task list consume the shared primitives; feature actions and detail content remain owned by their features.
- [x] #3 Inventory and reuse existing GUI table/filter/badge primitives where applicable; document the GUI/TUI boundary without forcing a common widget framework.
- [x] #4 Verify keyboard navigation, selection, focus, empty/loading/error states where applicable, long labels, narrow layouts and large lists against the canonical design-state matrix, recording evidence and explicit gaps; run required render-path performance checks if GUI paths change.
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implemented frontend-local list_presentation primitives with explicit viewport output and picker confirmation visibility output; Tasks and PRs consume shared cell/header/scroll layout. Feature details, paint and typed picker actions remain feature-owned. GUI primitives inventoried read-only in docs/tui-list-state-evidence.md with state matrix and explicit live/performance gaps. Targeted real-key/backlog/render E2Es: 43 passed, 13 opt-in live/evidence tests ignored. TUI all-target clippy -D warnings passed. Parent owns integration, installed TUI use and final commit.

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
