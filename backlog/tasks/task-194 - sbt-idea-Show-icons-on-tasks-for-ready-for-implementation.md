---
id: TASK-194
title: 'sbt idea: Show icons on tasks for ready for implementation'
status: To Do
assignee: []
created_date: '2026-09-08 16:16'
labels:
  - tui
  - idea
dependencies: []
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Filed from sbt 0.4.0 while at page=Tasks view=custom filter="status:!done" sort=id:ascending selected=TASK-141 pane=None.

Impact: Show icons on tasks for ready for implementation
Evidence: screen and action trail below, captured at filing time.

## Screen

```text
 [Tasks]   Pull Requests   tab switch page
PR #141: checks: Observed checks pass                                                        1 · n dismiss
┌ switchbard  custom · status:!done · ↑id · cols:id,status,priority,title,rank,ball · hide:done · group:p┐
│1 id  2 status    3 pri 4 title                                                               5 # 6 ball│
│▸ top · 2                                                                                               │
│141   To Do       M     sbt idea: PR page with actions and status notifications across pages  1   me    │
│80    To Do       H     Make the Task Queue aware of GitHub delivery state                    2         │
│▸ EGUI Polish · Planned · 0/2                                                                           │
│78    To Do       H     Elevation scale tokens in theme.rs                                              │
│79    To Do       M     Sweep surfaces onto the elevation scale                                         │
│▸ Abstraction · Planned · 0/7                                                                           │
│144   In Progress M     sbt idea: refactor keyboard shortcuts                                     agent │
│162   In Progress M     Extract reusable column definitions and sorting                           agent │
│163   In Progress M     Decouple filtering from task entities                                     agent │
│164   In Progress M     Extract reusable paint rule evaluation                                    agent │
│165   In Progress M     Make shared list UI components independent of whole-app state             agent │
│166   In Progress M     Generalize saved views across list features                               agent │
│167   In Progress M     Paint rows by when a PR merged and when a task was filed                  agent │
│▸ GitHub Operations · Planned · 0/6                                                                     │
│115   To Do       H     Lock the GitHub Operations authority, placement, and command contract           │
│116   To Do       H     Extend GitHub delivery observations for repository pull-request opera           │
│117   To Do       H     Build the canonical Ops > Pull requests surface                                 │
│118   To Do       H     Add guarded pull-request review operations                                      │
│119   To Do       H     Add guarded CI, branch-update, and merge operations                             │
│120   To Do       H     Dogfood and release GitHub Operations in the native app                         │
│▸ Instant Cold Start · Planned · 0/6                                                                    │
│121   To Do       H     Lock the instant-startup contract and failing first-frame journey               │
│122   To Do       H     Build the bounded sharded startup snapshot kernel                               │
│123   To Do       H     Render Servers immediately from last-known topology and processes               │
│124   To Do       H     Render Workspace immediately from last-known Git and worktree state             │
│125   To Do       H     Render Tasks and Dispatch immediately from last-known read models               │
│126   To Do       M     Unify Agents caching and close the cross-platform startup gates                 │
│▸ Views That Keep Themselves · Planned · 0/3                                                            │
│152   To Do       H     sbt: paint_auto stores a palette token, not a resolved hex                      │
│153   To Do       H     sbt: resume the last view on launch, not just across self-restart               │
│154   To Do       M     sbt: view history you can recognize - captured automatically, promote           │
│▸ no project                                                                                            │
│13    To Do       L     Virtualize Backlog task list rows for large repo/task counts                    │
│31    To Do       L     Tombstone filename collides on same-second consecutive wipes                    │
│36    To Do       L     Remove-repo confirmation can silently retarget between surfaces                 │
│38    To Do       M     Unify List/Milestones row selection with Board's stroke-based indicat           │
│39    To Do       M     Reap dispatch runs orphaned by an app restart                             me    │
│61    To Do       L     Landing worker: gh probe has no subprocess timeout                              │
│68    To Do       L     Format fork: diverge on named wins                                              │
│80.1  To Do       H     Prove the Task Queue with the Lucella delivery ledger                           │
│80.3  To Do       H     Build the Task Queue GitHub delivery backend                                    │
│80.4  To Do       H     Render mixed local and GitHub-backed work in the Task Queue                     │
└────────────────────────────────────────────────────────────────────────────────────────────────────────┘
:idea Show icons on tasks for ready for implementation▏
```

## Action trail

```text
action up (0.0ms)
action up (0.0ms)
action up (0.0ms)
action page (0.0ms)
action page (0.1ms)
action up (0.0ms)
action up (0.0ms)
action up (0.0ms)
action up (0.0ms)
action up (0.0ms)
action up (0.0ms)
action up (0.0ms)
action up (0.0ms)
action up (0.0ms)
action up (0.0ms)
action up (0.0ms)
action up (0.0ms)
action up (0.0ms)
action up (0.0ms)
action up (0.0ms)
action up (0.0ms)
action up (0.0ms)
action up (0.0ms)
unbound right
unbound right
unbound right
action up (0.0ms)
action up (0.0ms)
action up (0.0ms)
action command (0.0ms)
```
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Reporter confirms the behaviour in sbt matches what they were trying to do
<!-- AC:END -->
