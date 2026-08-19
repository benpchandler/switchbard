---
id: TASK-16
title: Cross-repo Statistics dashboard + burndown
status: Done
assignee: []
created_date: '2026-08-05 03:10'
updated_date: '2026-08-05 04:20'
labels:
  - hub
  - parity
dependencies:
  - TASK-15
priority: medium
ordinal: 16000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Tier-2 parity item pulled into the active workstream (owner 2026-08-04): totals, completion %, status and priority distributions, computed ACROSS all repos with per-repo breakdown, PLUS burndown info: completed-over-time vs remaining, overall and per-milestone. Data constraint: derive from existing task metadata (created/updated timestamps, status, completed/ source, milestone field) — no new stores, no schema changes. Note: Backlog v1.47 has no due-date field, so burndown is completion-trend, not due-date-based. Design per TASK-14 direction B (instrument-gauge header language).
<!-- SECTION:DESCRIPTION:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
compute_cross_repo_stats + compute_burndown/compute_burndown_by_milestone in switchbard-core/src/backlog_stats.rs — pure, no-IO functions over BacklogTask metadata already loaded by load_backlog_project (status, priority, created_date, updated_date, milestone), per this task's no-new-store constraint. Burndown is a completion-trend (day-bucketed cumulative created vs completed, using updated_date as the completion-day proxy for Done tasks) rather than due-date-based, since Backlog v1.47 has no due-date field — matches the same constraint backlog_triage.rs already documents for triage. GUI: new Statistics lens (crates/switchbard-gui/src/ui/backlog/stats.rs) with instrument-gauge header language (OPEN/DONE/TOTAL/REPOS/COMPLETION readouts) per TASK-14 direction B, status/priority distribution bars, a per-repo table, and overall + per-milestone burndown charts painted with plain egui Shape::line/rect_filled calls (no charting dependency).
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Totals, completion %, status/priority distribution (overall + per-repo), and completion-trend burndown (overall + per-milestone) all shipped as a fourth Backlog lens, derived entirely from already-cached task metadata. Covered by switchbard-core unit tests (backlog_stats.rs) and by legibility_audit (Statistics lens audited under both themes).
<!-- SECTION:FINAL_SUMMARY:END -->
