---
id: TASK-188
title: 'sbt bug: Help clips instructions at 80x24 and j moves hidden task selection instead of scrolling'
status: Done
assignee: []
created_date: '2026-09-08 14:35'
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

Impact: Help clips instructions at 80x24 and j moves hidden task selection instead of scrolling
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
:bug Help clips instructions at 80x24 and j moves hidden task selection instead
```

## Action trail

```text
session_start 0.4.0
action filter (0.1ms)
action filter_apply status:!done project:Abstraction
action help (0.0ms)
action command (0.0ms)
```
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Reporter confirms the behaviour in sbt matches what they were trying to do
- [x] #2 Full report instructions and lower actions are reachable with configured keyboard navigation at 80x24 and 40x8.
- [x] #3 Scrolling help preserves the selected task; top and reopening help reset position predictably.
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Impact: terminal users at 80x24 cannot read report guidance or discover lower help actions; navigation changes hidden task selection instead of exposing help. Medium priority because discoverability fails while task operations remain available. Evidence: agent-owned live sbt PTY at 80x24, ? screen clips the :idea instruction; failing real-key test shortcuts::narrow_help_keeps_report_instructions_accessible reproduces after 40 j events. Discovered while implementing the Abstraction project. Captured original screen and trail are retained in this sbt-filed report.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Implemented at code commit 815bcf01 on feat/tui-abstractions in /Users/bpc/Dev/.worktrees/switchbard-abstractions. Final isolated tui-install gate passed formatting, all-target clippy and 141 TUI tests; core suite passed 582 tests and core clippy. Seven opted-in live journeys passed, including authoritative GitHub date painting and a real live work claim. Installed and exercised /Users/bpc/.local/share/switchbard-builds/abstractions/bin/sbt in a real PTY. Existing default installation preserved because it contains an unmerged parent-picker feature. No push, PR or merge. Evidence: docs/tui-abstraction-mission.md and linked task-specific evidence documents. Reproduction and regression evidence are recorded in the implementation notes and relevant E2E suite.
<!-- SECTION:FINAL_SUMMARY:END -->
