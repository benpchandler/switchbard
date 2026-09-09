---
id: TASK-167
title: Paint rows by when a PR merged and when a task was filed
status: In Review
assignee: []
created_date: '2026-09-07 12:13'
updated_date: '2026-09-08 18:12'
labels:
  - tui
  - paint
  - dates
  - abstraction
  - ball:me
dependencies:
  - TASK-164
priority: medium
project: Abstraction
references:
  - https://github.com/benpchandler/switchbard/pull/145
  - https://github.com/benpchandler/switchbard/pull/144
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Impact: users triaging recent work cannot distinguish rows by merge time or task filing time through paint rules. Medium priority because this limits visual triage without blocking task operations.

Evidence: owner request on 2026-09-07: "Enable painting by When merged and/or when task filed." Repository inspection at 1668941: crates/switchbard-tui/src/columns.rs Column catalog has no date fields; paint.rs PaintRule::claim obtains categorical values through task-specific FilterField. TASK-164 already owns reusable paint evaluation and depends on TASK-162/TASK-163. Implement this feature using that planned abstraction work, coordinating with TASK-141.4 for PR painting and TASK-152 for palette tokens.

Scope: expose When merged and When task filed in existing painting controls. Use authoritative PR merge timestamps and task creation timestamps. Before implementation, settle date bucket/range vocabulary and timezone semantics in the task plan; do not equate task completion with PR merge or file modification with filing time.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Users can select When merged in existing PR painting controls and paint by the authoritative merge timestamp; unmerged or missing timestamps are explicit non-matches or an explicit missing-value category.
- [x] #2 Users can select When task filed in existing task painting controls, using task creation time and documented date range or age bucket semantics.
- [x] #3 Users can use either time-based rule independently and combine applicable rules; precedence follows the existing paint hierarchy and unsupported fields are not silently substituted.
- [x] #4 Implementation uses the shared field access and paint evaluation boundary from TASK-164 rather than adding a second date-specific paint engine.
- [x] #5 Saved views retain both time-based rule definitions across restart; existing rules, palette tokens and independent Tasks/PR settings remain compatible.
- [x] #6 Tests exercise date boundaries, timezone semantics, missing/invalid dates, overlapping rules, no matches, save/reload and narrow/wide paint pickers; record applicable design-state evidence and gaps.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Expose filed and merged date fields in the existing catalog and paint pickers. Use UTC calendar-day buckets: today, yesterday, last 7 days (including today), last 30 days (including today), older, future, missing. Relative ranges overlap intentionally and the existing rule order resolves precedence. Parse RFC3339 merge timestamps with offsets into UTC; timezone-free backlog creation timestamps follow the existing UTC storage convention. Missing/invalid timestamps never count as recent; unmerged PRs remain missing regardless of other activity. Persist bucket tokens in existing paint/view records. Verify boundaries, overlapping rules, missing/invalid values, restart isolation and narrow/wide pickers through real-key E2E journeys.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implemented When task filed and When merged in existing p controls; UTC calendar-day categories from Backlog created_date and typed authoritative GitHub mergedAt. Tests cover boundaries/missing/invalid/overlap, narrow and wide pickers, independent saved states and live GitHub row paint. Day-change invalidation refreshes both page projections, including cached offline PRs (TASK189). docs/tui-date-paint-evidence.md contains exact semantics and evidence. Root final gate/installation pending.

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
