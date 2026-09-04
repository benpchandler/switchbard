---
id: TASK-144
title: 'sbt idea: refactor keyboard shortcuts'
status: To Do
assignee: []
created_date: '2026-09-03 21:37'
labels:
  - tui
  - idea
dependencies: []
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Filed from sbt 0.4.0 while at view=custom filter="" sort= selected=TASK-96 pane=None.

Impact: refactor keyboard shortcuts
Evidence: screen and action trail below, captured at filing time.

## Screen

```text
┌ switchbard  custom · paint:1 · group:project · 106/106 ───────────────────────────────────────────────────────────────────┐
│1 id 2 status    3 pri 4 title                                                                                             │
│▸ Task Queue · Planned · 6/7                                                                                               │
│80   To Do       H     Make the Task Queue aware of GitHub delivery state                                                  │
│▸ Information Architecture V2 · Planned · 11/11                                                                            │
│76   Done        H     Lavish mockup: places-and-objects navigation                                                        │
│77   Done        H     Trajectory decision record for IA V2                                                                │
│96   Done        H     IA V2: sidebar shell - places nav, multi-select repo scope, favorites                               │
│97   Done        H     IA V2: Tasks place - generic grouping, filter builder, rank sort, expanding headers                 │
│98   Done        H     IA V2: Dispatches view + Command fleet console                                                      │
│99   Done        H     IA V2: Digest place - goal cards, in-flight, attention feed                                         │
│100  Done        M     IA V2: Ops place - merged Servers/Workspace, one row per worktree                                   │
│101  Done        M     IA V2: Goals place - index with inline check-in + goal page with Inputs card                        │
│105  Done        M     GUI: Tasks-place task titles clamp at two lines, never grow the row (List truncates to one, Board wr│
│106  Done        M     GUI: Tasks-place list title column and detail rail collide at narrow window widths, hiding task titl│
│107  Done        L     GUI: no UI path to create a saved view from the Tasks place's filter-builder predicates             │
│▸ EGUI Polish · Planned · 0/2                                                                                              │
│78   To Do       H     Elevation scale tokens in theme.rs                                                                  │
│79   To Do       M     Sweep surfaces onto the elevation scale                                                             │
│▸ GitHub Operations · Planned · 0/6                                                                                        │
│115  To Do       H     Lock the GitHub Operations authority, placement, and command contract                               │
│116  To Do       H     Extend GitHub delivery observations for repository pull-request operations                          │
│117  To Do       H     Build the canonical Ops > Pull requests surface                                                     │
│118  To Do       H     Add guarded pull-request review operations                                                          │
│119  To Do       H     Add guarded CI, branch-update, and merge operations                                                 │
│120  To Do       H     Dogfood and release GitHub Operations in the native app                                             │
│▸ Instant Cold Start · Planned · 0/6                                                                                       │
│121  To Do       H     Lock the instant-startup contract and failing first-frame journey                                   │
│122  To Do       H     Build the bounded sharded startup snapshot kernel                                                   │
│123  To Do       H     Render Servers immediately from last-known topology and processes                                   │
│124  To Do       H     Render Workspace immediately from last-known Git and worktree state                                 │
│125  To Do       H     Render Tasks and Dispatch immediately from last-known read models                                   │
│126  To Do       M     Unify Agents caching and close the cross-platform startup gates                                     │
│▸ no project                                                                                                               │
│127  In Progress H     Tasks place renders scoped rows reliably at current scale                                           │
│128  In Progress M     sb edit: remove or replace an acceptance criterion (--remove-ac N, --edit-ac N TEXT)                │
│80.3 To Do       H     Build the Task Queue GitHub delivery backend                                                        │
│80.4 To Do       H     Render mixed local and GitHub-backed work in the Task Queue                                         │
│80.1 To Do       H     Prove the Task Queue with the Lucella delivery ledger                                               │
│39   To Do       M     Reap dispatch runs orphaned by an app restart                                                       │
│81   To Do       M     RemovalAuthorization: make the force gate a domain type, not a caller-supplied bool                 │
│38   To Do       M     Unify List/Milestones row selection with Board's stroke-based indicator                             │
│13   To Do       L     Virtualize Backlog task list rows for large repo/task counts                                        │
│61   To Do       L     Landing worker: gh probe has no subprocess timeout                                                  │
│31   To Do       L     Tombstone filename collides on same-second consecutive wipes                                        │
│36   To Do       L     Remove-repo confirmation can silently retarget between surfaces                                     │
│68   To Do       L     Format fork: diverge on named wins                                                                  │
│87   To Do       H     Digest Tab: Clickable tasks                                                                         │
│110  To Do       H     Goal check-in drafts survive week rollover with stale values                                        │
│137  To Do       H     Owner cannot discover what is waiting on them without being told in chat                            │
│139  To Do       H     Owner cannot see at a glance which tasks an agent session is actively working                       │
│93   To Do       M     Give SB ability to detect refactoring candidates                                                    │
│94   To Do       M     Enable "integrations' vs hardcoded / config.                                                        │
│95   To Do       M     sb add <title>: quick capture that falls back to the hub repo outside a Backlog repo                │
│104  To Do       M     Create sprint from tasks / goals / projects                                                         │
│108  To Do       M     TASK-56's cross-thread repaint race recurs in other backlog_controls.rs tests                       │
│109  To Do       M     Retire the unreachable legacy Backlog lenses                                                        │
│113  To Do       M     Support-request store for Command (NEEDS_DECISION/SITREP)                                           │
│136  To Do       M     sbt: prior text on screen shows after terminal app quit and reload                                  │
│140  To Do       M     Id column truncation makes distinct ids look identical; the repeated repo prefix wastes the width   │
│141  To Do       M     sbt idea: PR page with actions and status notifications across pages                                │
│142  To Do       M     sbt idea: organize by goal                                                                          │
│143  To Do       M     sbt idea: when an idea comes in just start building it                                              │
└───────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────┘
:idea refactor keyboard shortcuts▏
```

## Action trail

```text
session_start 0.4.0
config_reload 0
action settings
action settings (0.0ms)
action command (0.0ms)
```
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Reporter confirms the behaviour in sbt matches what they were trying to do
<!-- AC:END -->
