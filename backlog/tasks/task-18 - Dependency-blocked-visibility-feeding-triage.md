---
id: TASK-18
title: Dependency + blocked visibility feeding triage
status: Done
assignee: []
created_date: '2026-08-05 03:55'
updated_date: '2026-08-05 05:15'
labels:
  - hub
  - beyond-parity
dependencies:
  - TASK-15
priority: high
ordinal: 18000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Detail shows depends-on/blocks both directions; computed blocked state (open deps) surfaces as marker in list/board and demotes/flags in triage_rank.
<!-- SECTION:DESCRIPTION:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Core: backlog_relations::is_blocked/blocking_dependencies/dependency_statuses/blocks — single-project-scoped (Backlog.md dependency ids carry no repo qualifier). triage_rank gained a blocked tiebreak, checked after priority and before age (importance still wins; blocked only matters once importance is tied) — see backlog_triage.rs's module doc for the reasoning. GUI: detail pane's Dependencies section now shows per-dependency done/open status plus an Unresolved line for dangling ids; new read-only Blocks section (the reverse edge); a 'blocked' marker (StatusKind::Danger / warn_orange, the same hot tone Operator's Console uses for alerts) on the detail header, List rows, and Board strips.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Both directions visible in detail; blocked marker on every surface a task appears; triage_rank demotes blocked tasks within their priority tier, with new unit tests (blocked_sinks_below_unblocked_within_the_same_priority_tier, blocked_does_not_override_a_higher_priority_tier). Covered by a kittest test exercising the full loop (marker, per-dependency status, reverse Blocks list) and by legibility_audit's fixture (a genuinely-blocked task in both themes).
<!-- SECTION:FINAL_SUMMARY:END -->
