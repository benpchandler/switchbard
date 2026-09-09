---
id: TASK-189
title: 'sbt bug: Date filters retain stale membership across UTC midnight while paint changes buckets'
status: Done
assignee: []
created_date: '2026-09-08 14:39'
updated_date: '2026-09-08 14:51'
labels:
  - tui
  - bug
dependencies: []
priority: medium
project: Bugs
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Filed from sbt 0.4.0 while at page=Tasks view=custom filter="status:!done project:Abstraction" sort= selected=TASK-162 pane=Help.

Impact: Date filters retain stale membership across UTC midnight while paint changes buckets
Evidence: screen and action trail below, captured at filing time.

## Screen

```text
 [Tasks]   Pull Requests   tab switch page
┌ keys ────────────────────────────────────────────────────────────────────────┐
│tab     page                    t n     new_task                              │
│down j  down                    k up    up                                    │
│g       top                     G       bottom                                │
│ctrl-d  page_down               ctrl-u  page_up                               │
│enter   open                    esc     back                                  │
│/       filter                  f       filter_column                         │
│s       sort_column             c       columns                               │
│p       paint                   b       ball                                  │
│w       pass                    o       group                                 │
│,       settings                t       task                                  │
│:       command                 r       reload                                │
│O       open_browser            m       merge                                 │
│n       dismiss_notifications   ?       help                                  │
│v       view                    q       quit                                  │
│t a     link parent task        1-9     column actions                        │
│v1      status:!done group:project [repo]v2      status:todo                  │
│v3      status:inprogress       v4      label:tui                             │
│v5      ball:me                                                               │
│                                                                              │
│:bug <doing>  file a bug with this screen    :idea <want>  file an idea with t│
└──────────────────────────────────────────────────────────────────────────────┘
:bug Date filters retain stale membership across UTC midnight while paint change
```

## Action trail

```text
session_start 0.4.0
action filter (0.1ms)
action filter_apply status:!done project:Abstraction
action help (0.0ms)
action command (0.0ms)
report Bug TASK-188
action command bug (482.9ms)
action command (0.0ms)
```
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Both Tasks and cached PR projections refresh relative-date membership on a UTC day change, including offline PR observations.
- [x] #2 A real-key disk-backed regression verifies date invalidation without editing a task or changing its filter.
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Impact: users leaving relative-date views open across UTC midnight can see rows that no longer match the filter while those rows paint in a different bucket. Medium priority because triage becomes misleading without changing stored tasks or PRs. Evidence: independent source review of date_fields.rs UTC sampling versus App::tick projection invalidation during TASK-167 integration; subsequent disk-backed real-key regression covers day invalidation. The attached filing screen records the report context, not an observed wall-clock midnight. Fix is part of this mission.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Implemented at code commit 815bcf01 on feat/tui-abstractions in /Users/bpc/Dev/.worktrees/switchbard-abstractions. Final isolated tui-install gate passed formatting, all-target clippy and 141 TUI tests; core suite passed 582 tests and core clippy. Seven opted-in live journeys passed, including authoritative GitHub date painting and a real live work claim. Installed and exercised /Users/bpc/.local/share/switchbard-builds/abstractions/bin/sbt in a real PTY. Existing default installation preserved because it contains an unmerged parent-picker feature. No push, PR or merge. Evidence: docs/tui-abstraction-mission.md and linked task-specific evidence documents. Reproduction and regression evidence are recorded in the implementation notes and relevant E2E suite.
<!-- SECTION:FINAL_SUMMARY:END -->
