---
id: TASK-147
title: 'sbt idea: switchbard persists all data rather than in repo; maybe writes some kind of summary file in repo?'
status: To Do
assignee: []
created_date: '2026-09-03 22:32'
labels:
  - tui
  - idea
dependencies: []
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Filed from sbt 0.4.0 while at view=custom filter="" sort= selected=TASK-128 pane=None.

Impact: switchbard persists all data rather than in repo; maybe writes some kind of summary file in repo?
Evidence: screen and action trail below, captured at filing time.

## Screen

```text
┌ switchbard  custom · cols:status,priority,title,rank · hide:done · paint:1 · group:project · 51/109 ──────────────────────┐
│1 status    2 pri 3 title                                                                                               4 #│
│▸ top 5 · 5/5                                                                                                              │
│To Do       H     Make the Task Queue aware of GitHub delivery state                                                    1  │
│In Progress H     Tasks place renders scoped rows reliably at current scale                                             2  │
│To Do       M     sbt idea: organize by goal                                                                            3  │
│To Do       M     sbt idea: PR page with actions and status notifications across pages                                  4  │
│In Progress M     sb edit: remove or replace an acceptance criterion (--remove-ac N, --edit-ac N TEXT)                  5  │
│▸ EGUI Polish · Planned · 0/2                                                                                              │
│To Do       H     Elevation scale tokens in theme.rs                                                                       │
│To Do       M     Sweep surfaces onto the elevation scale                                                                  │
│▸ GitHub Operations · Planned · 0/6                                                                                        │
│To Do       H     Lock the GitHub Operations authority, placement, and command contract                                    │
│To Do       H     Extend GitHub delivery observations for repository pull-request operations                               │
│To Do       H     Build the canonical Ops > Pull requests surface                                                          │
│To Do       H     Add guarded pull-request review operations                                                               │
│To Do       H     Add guarded CI, branch-update, and merge operations                                                      │
│To Do       H     Dogfood and release GitHub Operations in the native app                                                  │
│▸ Instant Cold Start · Planned · 0/6                                                                                       │
│To Do       H     Lock the instant-startup contract and failing first-frame journey                                        │
│To Do       H     Build the bounded sharded startup snapshot kernel                                                        │
│To Do       H     Render Servers immediately from last-known topology and processes                                        │
│To Do       H     Render Workspace immediately from last-known Git and worktree state                                      │
│To Do       H     Render Tasks and Dispatch immediately from last-known read models                                        │
│To Do       M     Unify Agents caching and close the cross-platform startup gates                                          │
│▸ no project                                                                                                               │
│To Do       H     Build the Task Queue GitHub delivery backend                                                             │
│To Do       H     Render mixed local and GitHub-backed work in the Task Queue                                              │
│To Do       H     Prove the Task Queue with the Lucella delivery ledger                                                    │
│To Do       M     Reap dispatch runs orphaned by an app restart                                                            │
│To Do       M     RemovalAuthorization: make the force gate a domain type, not a caller-supplied bool                      │
│To Do       M     Unify List/Milestones row selection with Board's stroke-based indicator                                  │
│To Do       L     Virtualize Backlog task list rows for large repo/task counts                                             │
│To Do       L     Landing worker: gh probe has no subprocess timeout                                                       │
│To Do       L     Tombstone filename collides on same-second consecutive wipes                                             │
│To Do       L     Remove-repo confirmation can silently retarget between surfaces                                          │
│To Do       L     Format fork: diverge on named wins                                                                       │
│To Do       H     Digest Tab: Clickable tasks                                                                              │
│To Do       H     Goal check-in drafts survive week rollover with stale values                                             │
│To Do       H     Owner cannot discover what is waiting on them without being told in chat                                 │
│To Do       H     Owner cannot see at a glance which tasks an agent session is actively working                            │
│To Do       M     Give SB ability to detect refactoring candidates                                                         │
│To Do       M     Enable "integrations' vs hardcoded / config.                                                             │
│To Do       M     sb add <title>: quick capture that falls back to the hub repo outside a Backlog repo                     │
│To Do       M     Create sprint from tasks / goals / projects                                                              │
│To Do       M     TASK-56's cross-thread repaint race recurs in other backlog_controls.rs tests                            │
│To Do       M     Retire the unreachable legacy Backlog lenses                                                             │
│To Do       M     Support-request store for Command (NEEDS_DECISION/SITREP)                                                │
│To Do       M     sbt: prior text on screen shows after terminal app quit and reload                                       │
│To Do       M     Id column truncation makes distinct ids look identical; the repeated repo prefix wastes the width        │
│To Do       M     sbt idea: when an idea comes in just start building it                                                   │
│To Do       M     sbt idea: refactor keyboard shortcuts                                                                    │
│To Do       M     sbt idea: rename group to outline app wide to match o                                                    │
│To Do       M     sbt idea: make evidence visisble                                                                         │
│To Do       L     Goal page history card recomputes statuses per week per frame                                            │
│To Do       L     Digest sections disagree on repo scope when a single repo is drilled in                                  │
│To Do       L     Perf-doc staleness sweep from IA V2                                                                      │
│Icebox      M     On Agent Context tab, enable quick actions like delete for commands, hooks, skills, etc                  │
│                                                                                                                           │
│                                                                                                                           │
│                                                                                                                           │
│                                                                                                                           │
│                                                                                                                           │
└───────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────┘
:idea switchbard persists all data rather than in repo; maybe writes some kind of summary file in repo?▏
```

## Action trail

```text
action down (0.0ms)
action down (0.0ms)
action down (0.0ms)
action down (0.0ms)
action down (0.0ms)
action down (0.0ms)
action up (0.0ms)
action down (0.0ms)
action column_actions rank
action rank (0.0ms)
action rank 4 TASK-141
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
action rank (0.0ms)
action rank 5 TASK-128
unbound space
action command (0.0ms)
```
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Reporter confirms the behaviour in sbt matches what they were trying to do
<!-- AC:END -->
