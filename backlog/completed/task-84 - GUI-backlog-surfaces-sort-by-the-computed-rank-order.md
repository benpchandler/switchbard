---
id: TASK-84
title: 'GUI: backlog surfaces sort by the computed rank order'
status: Done
assignee: []
created_date: '2026-08-31 22:01'
updated_date: '2026-09-01 00:12'
labels:
  - backlog
  - gui
dependencies:
  - TASK-82
priority: high
project: Stack Ranking
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Read-only GUI half first (trajectory: 'Stack ranking'). Backlog views (project list, task lists within a project, sub-issue lists) sort by the computed next-up order from backlog/ordering.rs: expedite lane first where a flat task view exists, ranked items in rank order, unranked items by the existing comparator. Expedited tasks get a visible marker (theme.rs semantic color/glyph). No reorder controls in this slice. Respect render-path perf discipline: ordering comes from the snapshot, no per-frame file reads.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Projects and tasks render in computed rank order across backlog surfaces; unranked items keep today's relative order
- [x] #2 Expedited tasks carry a visible marker sourced from theme.rs
- [x] #3 SWITCHBARD_PERF smoke shows no p95 regression vs previous build on the touched surfaces
<!-- AC:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Projects lens sorts by the computed rank order: ranked projects lead their initiative group in rank order (unranked keep the name sort), group rows and the Unassigned bucket re-sort by repo.tasks position (the authority load_backlog_repo already sorted), and expedited rows wear a theme-semantic 'expedited' pill (StatusKind::Danger) plus the same pill in the detail rail - no click needed. The flat List lens's Triage sort is deliberately untouched (cross-repo rank composition is trajectory-deferred). Evidence: GUI unit tests (ranked_projects_lead_their_group_and_carry_rank_facts, group_rows_follow_the_repos_computed_order), accesskit behavior test (projects_lens_renders_the_expedite_marker_and_lane_toggle), and perf: sorting itself costs ~0.4ms p95 at a worst-case 400-rows-all-ranked fixture (A/B in tests/projects_rank_perf_smoke.rs; ledger docs/perf/runs/2026-09-01-projects-rank-smoke.json).
<!-- SECTION:FINAL_SUMMARY:END -->
