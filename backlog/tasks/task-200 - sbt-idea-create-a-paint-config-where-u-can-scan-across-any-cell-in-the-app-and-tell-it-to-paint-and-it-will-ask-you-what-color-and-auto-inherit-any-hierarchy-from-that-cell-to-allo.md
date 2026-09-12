---
id: TASK-200
title: 'sbt idea: create a paint config where u can scan across any cell in the app and tell it to paint and it will ask you what color and auto-inherit any hierarchy  from that cell to allow you to paint a bunch of stuff simultaneously like rules. should incl header rows, etc.'
status: To Do
assignee: []
created_date: '2026-09-08 18:09'
labels:
  - tui
  - idea
dependencies: []
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Filed from sbt 0.4.0 while at page=PullRequests view=v1 filter="" sort= selected=TASK-197 pane=None.

Impact: create a paint config where u can scan across any cell in the app and tell it to paint and it will ask you what color and auto-inherit any hierarchy  from that cell to allow you to paint a bunch of stuff simultaneously like rules. should incl header rows, etc.
Evidence: screen and action trail below, captured at filing time.

## Screen

```text
 Tasks   [Pull Requests]   1   Inbox   tab switch page
PR #144: checks: Checks unknown                                                              5 · n dismiss
┌ Pull Requests ─────────────────────────────────────────────────────────────────────────────────────────┐
│benpchandler/switchbard · Observed 47s                                                                  │
│100/100 shown · PARTIAL · :more                                                                         │
│filter: all                                                                                             │
│                                                                                                        │
│1 id    2 State 3 Tasks      4 Checks       5 title                                                     │
│#144    Open    TASK-193     Unknown        Show open PR counts and add an Inbox destination            │
│#145    Merged  7 linked     NF             feat(tui): deliver shared abstractions and Inbox navigation │
│#143    Merged  2 linked     NF             Choose parent tasks by ID and title in the TUI              │
│#142    Merged  -            NF             Consolidate TUI footer menus into shared pickers; add task c│
│#141    Merged  -            NF             Fix six reported bugs; route CI by change scope             │
│#140    Merged  -            NF             Add task creation and consolidate TUI menus into pickers    │
│#139    Merged  -            NF             Link PR 138 to its related backlog tasks                    │
│#138    Merged  -            NF             Add guarded PR merging and cross-page alerts to sbt         │
│#137    Merged  -            NF             Track sent review feedback failing to resume Codex          │
│#136    Merged  4 linked     NF             feat(tui): add task-parity pull request side pane           │
│#135    Merged  -            NF             [codex] Support named Ball holders in sbt                   │
│#134    Merged  TASK-150     NF             feat: live work marker - sbt lights the rows a session is wo│
│#133    Merged  -            NF             backlog: TASK-142 done, reporter confirmed                  │
│#132    Merged  -            NF             feat(tui): organize by both - goal nested inside project, an│
│#131    Merged  -            NF             feat(tui): o picks what to organize by; tg opens the goal pa│
│#130    Merged  -            NF             feat(tui): link a task to a goal from sbt - a panel and :goa│
│#129    Merged  -            NF             feat(tui): goal column - filter, group, and show the weekly │
│#128    Merged  -            NF             fix(gui): refuse task writes against a stale source; close T│
│#127    Merged  -            NF             backlog: TASK-147 and top list order                        │
│#126    Merged  -            NF             feat(tui): top list of any length                           │
│#125    Merged  -            NF             backlog: sbt ideas TASK-141..146, TASK-80 ranked            │
│#124    Closed  -            NF             feat(tui): Top 5 queue (t1-t5, td, tp) on the expedite lane │
│#123    Closed  -            NF             feat(tui): :bug/:idea file into a configured repo           │
│#122    Closed  -            NF             feat(tui): settings panel, hide statuses everywhere         │
│#121    Closed  -            NF             feat(tui): per-column abbreviation toggle                   │
│#120    Merged  -            NF             feat(tui): content-fit columns, bare ids, H/M/L priority    │
│#119    Merged  -            NF             backlog: TASK-138 done                                      │
│#118    Merged  -            NF             feat(tui): berg palette is Bloomberg's categorical hues; chi│
│#117    Merged  -            NF             feat(tui): theme as configurable surfaces (Bloomberg-shaped)│
│#116    Merged  -            NF             feat(tui): palette presets, :palette live swap, visible auto│
│#115    Merged  -            NF             fix(tui): group headings contrast with painted rows         │
│#114    Merged  -            NF             backlog: TASK-138 in review                                 │
│#113    Merged  -            NF             ci: prune cargo cache with rust-cache; never cancel the main│
│#112    Merged  -            NF             feat(sb): --ball, project rename follow-through, --parent (T│
│#111    Merged  -            NF             feat(tui): column actions on the header digit               │
│#110    Merged  -            NF             backlog: close out sbt refactoring tasks TASK-129..132      │
│#109    Merged  -            NF             refactor(tui): split app.rs by concept and tests by feature │
│#108    Merged  -            NF             refactor(tui): one column catalog (TASK-132)                │
│#107    Merged  -            NF             refactor(tui): one ViewState for App, slots, and resume (TAS│
└────────────────────────────────────────────────────────────────────────────────────────────────────────┘
:idea create a paint config where u can scan across any cell in the app and tell it to paint and it will a
```

## Action trail

```text
session_start 0.4.0
unbound i
unbound d
unbound e
unbound a
unbound i
unbound d
unbound e
unbound a
action back (0.3ms)
action back (0.0ms)
unbound i
unbound d
unbound e
unbound a
action back (0.0ms)
action back (0.0ms)
action back (0.0ms)
action page (0.1ms)
action up (0.0ms)
action up (0.0ms)
action up (0.0ms)
action up (0.0ms)
action down (0.0ms)
action merge (0.1ms)
action up (0.0ms)
action merge (0.0ms)
action command (0.0ms)
```
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Reporter confirms the behaviour in sbt matches what they were trying to do
<!-- AC:END -->
