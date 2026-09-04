---
id: TASK-151
title: 'sbt idea: migrate agents to graph'
status: To Do
assignee: []
created_date: '2026-09-04 10:51'
labels:
  - tui
  - idea
dependencies: []
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Filed from sbt 0.4.0 while at view=custom filter="" sort= selected=TASK-86 pane=None.

Impact: migrate agents to graph
Evidence: screen and action trail below, captured at filing time.

## Screen

```text
┌ switchbard  custom · cols:status,rank,title,priority,id · hide:done · paint:1 · group:goal›pro┐
│1 status    2 # 3 title                                                              4 pri 5 id│
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
│  ▸ no project                                                                                 │
│In Progress     Live work marker: sbt blinks the rows an agent session is working; s H     150 │
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
│To Do           sbt: prior text on screen shows after terminal app quit and reload   M     136 │
│To Do           Id column truncation makes distinct ids look identical; the repeated M     140 │
│To Do           sbt idea: when an idea comes in just start building it               M     143 │
│To Do           sbt idea: refactor keyboard shortcuts                                M     144 │
│To Do           sbt idea: rename group to outline app wide to match o                M     145 │
│To Do           sbt idea: make evidence visisble                                     M     146 │
│To Do           sbt idea: switchbard persists all data rather than in repo; maybe wr M     147 │
│To Do           sbt idea: when ranking a task, keep the cursor where the task was ra M     148 │
│To Do           sbt idea: add a PR column ties to the tasks so u can see what's goin M     149 │
│To Do           Goal page history card recomputes statuses per week per frame        L     111 │
│To Do           Digest sections disagree on repo scope when a single repo is drilled L     112 │
│To Do           Perf-doc staleness sweep from IA V2                                  L     114 │
│Icebox          On Agent Context tab, enable quick actions like delete for commands, M     86  │
└───────────────────────────────────────────────────────────────────────────────────────────────┘
:idea migrate agents to graph▏
```

## Action trail

```text
action down (0.0ms)
action down (0.0ms)
action down (0.0ms)
action down (0.0ms)
action down (0.0ms)
action down (0.0ms)
action down (0.0ms)
action down (0.0ms)
action down (0.0ms)
action down (0.0ms)
action down (0.0ms)
action down (0.0ms)
action down (0.0ms)
action down (0.0ms)
action down (0.0ms)
action down (0.0ms)
action down (0.0ms)
action down (0.0ms)
action down (0.0ms)
action down (0.0ms)
action down (0.0ms)
action down (0.0ms)
action down (0.0ms)
action down (0.0ms)
action down (0.0ms)
action down (0.0ms)
action down (0.0ms)
action down (0.0ms)
action down (0.0ms)
action command (0.0ms)
```
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Reporter confirms the behaviour in sbt matches what they were trying to do
<!-- AC:END -->
