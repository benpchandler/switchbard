---
id: TASK-167
title: Paint rows by when a PR merged and when a task was filed
status: To Do
assignee: []
created_date: '2026-09-07 12:13'
labels:
  - tui
  - paint
  - dates
  - abstraction
dependencies:
  - TASK-164
priority: medium
project: Abstraction
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Impact: users triaging recent work cannot distinguish rows by merge time or task filing time through paint rules. Medium priority because this limits visual triage without blocking task operations.

Evidence: owner request on 2026-09-07: "Enable painting by When merged and/or when task filed." Repository inspection at 1668941: crates/switchbard-tui/src/columns.rs Column catalog has no date fields; paint.rs PaintRule::claim obtains categorical values through task-specific FilterField. TASK-164 already owns reusable paint evaluation and depends on TASK-162/TASK-163. Implement this feature using that planned abstraction work, coordinating with TASK-141.4 for PR painting and TASK-152 for palette tokens.

Scope: expose When merged and When task filed in existing painting controls. Use authoritative PR merge timestamps and task creation timestamps. Before implementation, settle date bucket/range vocabulary and timezone semantics in the task plan; do not equate task completion with PR merge or file modification with filing time.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Users can select When merged in existing PR painting controls and paint by the authoritative merge timestamp; unmerged or missing timestamps are explicit non-matches or an explicit missing-value category.
- [ ] #2 Users can select When task filed in existing task painting controls, using task creation time and documented date range or age bucket semantics.
- [ ] #3 Users can use either time-based rule independently and combine applicable rules; precedence follows the existing paint hierarchy and unsupported fields are not silently substituted.
- [ ] #4 Implementation uses the shared field access and paint evaluation boundary from TASK-164 rather than adding a second date-specific paint engine.
- [ ] #5 Saved views retain both time-based rule definitions across restart; existing rules, palette tokens and independent Tasks/PR settings remain compatible.
- [ ] #6 Tests exercise date boundaries, timezone semantics, missing/invalid dates, overlapping rules, no matches, save/reload and narrow/wide paint pickers; record applicable design-state evidence and gaps.
<!-- AC:END -->
