---
id: TASK-156
title: 'sbt idea: improve onboarding'
status: To Do
assignee: []
created_date: '2026-09-04 12:42'
labels:
  - tui
  - idea
dependencies: []
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Filed from sbt 0.4.0 while at view=custom filter="" sort= selected=TASK-78 pane=None.

Impact: improve onboarding
Evidence: screen and action trail below, captured at filing time.

## Screen

```text
┌ switchbard  custom · cols:status,rank,title,priority,id · hide:done · group:goal›project · wor┐
│1 status    2 # 3 title                                                              4 pri 5 id│
│▸ top · 3                                                                                      │
│To Do       1   Make the Task Queue aware of GitHub delivery state                   H     80  │
│To Do       2   sbt idea: PR page with actions and status notifications across pages M     141 │
│In Progress 3   Live work marker: sbt blinks the rows an agent session is working; s H     150 │
│▸ no goal                                                                                      │
│  ▸ EGUI Polish · Planned · 0/2                                                                │
│To Do           Elevation scale tokens in theme.rs                                   H     78  │
│To Do           Sweep surfaces onto the elevation scale                              M     79  │
│  ▸ GitHub Operations · Planned · 0/6                                                          │
│To Do           Lock the GitHub Operations authority, placement, and command contrac H     115 │
│To Do           Extend GitHub delivery observations for repository pull-request oper H     116 │
│To Do           Build the canonical Ops > Pull requests surface                      H     117 │
│To Do           Add guarded pull-request review operations                           H     118 │
│To Do           Add guarded CI, branch-update, and merge operations                  H     119 │
│To Do           Dogfood and release GitHub Operations in the native app              H     120 │
│  ▸ Instant Cold Start · Planned · 0/6                                                         │
│To Do           Lock the instant-startup contract and failing first-frame journey    H     121 │
│To Do           Build the bounded sharded startup snapshot kernel                    H     122 │
│To Do           Render Servers immediately from last-known topology and processes    H     123 │
│To Do           Render Workspace immediately from last-known Git and worktree state  H     124 │
│To Do           Render Tasks and Dispatch immediately from last-known read models    H     125 │
│To Do           Unify Agents caching and close the cross-platform startup gates      M     126 │
│  ▸ Views That Keep Themselves · Planned · 0/3                                                 │
│To Do           sbt: paint_auto stores a palette token, not a resolved hex           H     152 │
│To Do           sbt: resume the last view on launch, not just across self-restart    H     153 │
│To Do           sbt: view history you can recognize - captured automatically, promot M     154 │
│  ▸ no project                                                                                 │
│To Do           Build the Task Queue GitHub delivery backend                         H     80.3│
│To Do           Render mixed local and GitHub-backed work in the Task Queue          H     80.4│
│To Do           Prove the Task Queue with the Lucella delivery ledger                H     80.1│
│To Do           Reap dispatch runs orphaned by an app restart                        M     39  │
│To Do           RemovalAuthorization: make the force gate a domain type, not a calle M     81  │
│To Do           Unify List/Milestones row selection with Board's stroke-based indica M     38  │
│To Do           Virtualize Backlog task list rows for large repo/task counts         L     13  │
│To Do           Landing worker: gh probe has no subprocess timeout                   L     61  │
│To Do           Tombstone filename collides on same-second consecutive wipes         L     31  │
│To Do           Remove-repo confirmation can silently retarget between surfaces      L     36  │
│To Do           Format fork: diverge on named wins                                   L     68  │
│To Do           Digest Tab: Clickable tasks                                          H     87  │
│To Do           Goal check-in drafts survive week rollover with stale values         H     110 │
│To Do           Owner cannot discover what is waiting on them without being told in  H     137 │
│To Do           Owner cannot see at a glance which tasks an agent session is activel H     139 │
│To Do           Give SB ability to detect refactoring candidates                     M     93  │
│To Do           Enable "integrations' vs hardcoded / config.                         M     94  │
│To Do           sb add <title>: quick capture that falls back to the hub repo outsid M     95  │
│To Do           Create sprint from tasks / goals / projects                          M     104 │
│To Do           TASK-56's cross-thread repaint race recurs in other backlog_controls M     108 │
│To Do           Retire the unreachable legacy Backlog lenses                         M     109 │
│To Do           Support-request store for Command (NEEDS_DECISION/SITREP)            M     113 │
└───────────────────────────────────────────────────────────────────────────────────────────────┘
:idea improve onboarding▏
```

## Action trail

```text
session_start 0.4.0
action paint
action paint (1.5ms)
action paint_clear_all
unbound left
unbound left
unbound left
unbound left
unbound left
unbound left
action filter (0.0ms)
action filter_cancel
config_reload 0
action reload (6.1ms)
action command (0.0ms)
```
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Reporter confirms the behaviour in sbt matches what they were trying to do
<!-- AC:END -->
