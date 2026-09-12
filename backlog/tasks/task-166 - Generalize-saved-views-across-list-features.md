---
id: TASK-166
title: Generalize saved views across list features
status: In Review
assignee: []
created_date: '2026-09-07 10:05'
updated_date: '2026-09-08 18:12'
labels:
  - abstraction
  - refactor
  - views
  - persistence
  - ball:me
dependencies:
  - TASK-162
  - TASK-163
  - TASK-164
priority: medium
project: Abstraction
references:
  - https://github.com/benpchandler/switchbard/pull/145
  - https://github.com/benpchandler/switchbard/pull/144
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Impact: developers adding list features must reproduce task-specific view persistence, risking lost settings or cross-page leakage for users. Priority is medium because current task persistence exists but does not offer an entity-neutral contract.

Evidence: crates/switchbard-tui/src/views.rs:19 ViewState bundles filter, sort, columns, glyphs, paint and grouping but imports task Column/Sort/Grouping types. Verified with sed -n "1,75p" on views.rs. Extend the single ViewState authority from TASK-130 and reconcile TASK-141.1; TASK-153 and TASK-154 retain startup-resume and history outcomes.

Discovery: owner-requested reuse assessment in session 01a07c1d-7259-73d1-82b7-97d68ba8ac87, recovered 2026-09-07; source verified at 7d09874 with rg/sed. Reconcile the implementation behind TASK-141.1 through TASK-141.4 before changing code; their PR parity behavior is not new scope here. Preserve existing commands and saved data. Choose between a small shared core contract and frontend-local adapters based on two concrete consumers; document the boundary and rationale before extraction, without a speculative registration framework.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Represent reusable view settings against explicit list/column identities with feature capabilities and separate per-feature scope.
- [x] #2 Tasks and an existing non-task list round-trip applicable filter, sort, columns, glyphs, paint and grouping settings without cross-page leakage.
- [x] #3 Existing saved task views load compatibly; missing/renamed/unsupported columns and malformed or older records have a documented, tested non-destructive policy.
- [x] #4 Slot save/load and restart tests prove compatibility and feature isolation; extend the existing serialization authority and coordinate with TASK-153/TASK-154 rather than creating a second persistence store.
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implemented explicit ListSettings(Page) identity/capability/file scope over existing ViewState/Store. Compatibility tests cover aliases, missing files, malformed/future fields, invalid paint, external edits, sparse numbered slots, and retry after partial promotion; no second persistence format or source. Independent read-only review found no remaining concrete blockers after TASK191/192 repairs. Evidence docs/tui-view-scope-evidence.md. Root final gate/installation pending.

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
