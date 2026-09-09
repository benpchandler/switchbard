---
id: TASK-190
title: 'sbt bug: One-row grouped viewport hides selected task behind its heading'
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

Impact: One-row grouped viewport hides selected task behind its heading
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
:bug One-row grouped viewport hides selected task behind its heading▏
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
```
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 A grouped list with one visible body slot renders its selected task instead of the preceding heading.
- [x] #2 Real-key resize/navigation regression covers one and two body slots and preserves selection.
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Impact: users in a short terminal cannot see the selected task when a grouped list has exactly one body row, preventing reliable keyboard navigation. Medium priority because resizing avoids the issue and no data changes. Evidence: independent review of ListViewport::new heading retention at slots==1; reproduced through a real-key grouped task E2E journey. Filed from sbt while implementing TASK-165; the filing screen is context rather than the small-viewport reproduction.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Implemented at code commit 815bcf01 on feat/tui-abstractions in /Users/bpc/Dev/.worktrees/switchbard-abstractions. Final isolated tui-install gate passed formatting, all-target clippy and 141 TUI tests; core suite passed 582 tests and core clippy. Seven opted-in live journeys passed, including authoritative GitHub date painting and a real live work claim. Installed and exercised /Users/bpc/.local/share/switchbard-builds/abstractions/bin/sbt in a real PTY. Existing default installation preserved because it contains an unmerged parent-picker feature. No push, PR or merge. Evidence: docs/tui-abstraction-mission.md and linked task-specific evidence documents. Reproduction and regression evidence are recorded in the implementation notes and relevant E2E suite.
<!-- SECTION:FINAL_SUMMARY:END -->
