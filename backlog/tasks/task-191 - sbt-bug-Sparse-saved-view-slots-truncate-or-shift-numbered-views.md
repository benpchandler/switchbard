---
id: TASK-191
title: 'sbt bug: Sparse saved view slots truncate or shift numbered views'
status: Done
assignee: []
created_date: '2026-09-08 14:44'
updated_date: '2026-09-08 14:51'
labels:
  - tui
  - bug
dependencies: []
priority: high
project: Bugs
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Filed from sbt 0.4.0 while at page=Tasks view=custom filter="status:!done project:Abstraction" sort= selected=TASK-162 pane=Help.

Impact: Sparse saved view slots truncate or shift numbered views
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
:bug Sparse saved view slots truncate or shift numbered views▏
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
report Bug TASK-189
action command bug (447.9ms)
action command (0.0ms)
report Bug TASK-190
action command bug (427.7ms)
action command (0.0ms)
```
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Global Lua records with holes are rejected with original bytes preserved during save and promotion.
- [x] #2 Sparse repo overrides keep their explicit slot numbers in picker labels, loading, saving and restart.
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Impact: users with sparse saved slots can lose later global view definitions on save or reopen a different numbered repo view. High priority because saved user configuration can be silently overwritten. Evidence: independent vendored-mlua probe return { {}, nil, {}, {} } gives raw_len 4 but sequence_count 1; ViewStore slots used a compacting filter_map. Discovered during TASK-166 review; disk-backed real-key regressions follow. The sbt filing screen records review context, not the sparse-file reproduction.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Implemented at code commit 815bcf01 on feat/tui-abstractions in /Users/bpc/Dev/.worktrees/switchbard-abstractions. Final isolated tui-install gate passed formatting, all-target clippy and 141 TUI tests; core suite passed 582 tests and core clippy. Seven opted-in live journeys passed, including authoritative GitHub date painting and a real live work claim. Installed and exercised /Users/bpc/.local/share/switchbard-builds/abstractions/bin/sbt in a real PTY. Existing default installation preserved because it contains an unmerged parent-picker feature. No push, PR or merge. Evidence: docs/tui-abstraction-mission.md and linked task-specific evidence documents. Reproduction and regression evidence are recorded in the implementation notes and relevant E2E suite.
<!-- SECTION:FINAL_SUMMARY:END -->
