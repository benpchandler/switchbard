---
id: TASK-162
title: Extract reusable column definitions and sorting
status: In Review
assignee: []
created_date: '2026-09-07 10:05'
updated_date: '2026-09-08 18:12'
labels:
  - abstraction
  - refactor
  - sorting
  - columns
  - ball:me
dependencies: []
priority: medium
project: Abstraction
references:
  - https://github.com/benpchandler/switchbard/pull/145
  - https://github.com/benpchandler/switchbard/pull/144
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Impact: developers adding non-task lists must duplicate task-specific column access and ordering, allowing users to see inconsistent sorting and grouping across features. Priority is medium because this is an extensibility and consistency gap, not a demonstrated outage.

Evidence: crates/switchbard-tui/src/columns.rs:28 defines ColumnSpec, but values/cell_text/display_text at lines 226/249/264 take BacklogTask; src/sort.rs owns task ordering. Verified using rg -n "BacklogTask|pub struct" on columns.rs. Extend the TASK-132 catalog rather than replacing it with a competing authority.

Discovery: owner-requested reuse assessment in session 01a07c1d-7259-73d1-82b7-97d68ba8ac87, recovered 2026-09-07; source verified at 7d09874 with rg/sed. Reconcile the implementation behind TASK-141.1 through TASK-141.4 before changing code; their PR parity behavior is not new scope here. Preserve existing commands and saved data. Choose between a small shared core contract and frontend-local adapters based on two concrete consumers; document the boundary and rationale before extraction, without a speculative registration framework.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Define explicit column identities, value accessors and capabilities usable by Tasks and one existing non-task list, with feature-specific labels and semantic order retained.
- [x] #2 Both consumers use shared sorting logic with deterministic ties, ascending/descending behavior, missing values, and categorical/numeric semantics covered by tests.
- [x] #3 Column display, filtering, sorting and grouping consult the same capability definitions; multi-valued/non-groupable columns remain explicit.
- [x] #4 Existing task column configuration, rank ordering and grouping remain compatible; document how to add one new column without duplicating shared algorithms.
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implemented in feat/tui-abstractions: explicit TaskValues/PrValues adapters, shared comparator with deterministic numeric/identity ties, shared capability catalog including numeric and multi-valued properties. Verified existing sorting/grouping/rank and six live authenticated read-only PR journeys. Boundary rationale and adding-column guide: docs/tui-abstraction-boundaries.md. Root integration and isolated installed-binary verification still in progress.

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
