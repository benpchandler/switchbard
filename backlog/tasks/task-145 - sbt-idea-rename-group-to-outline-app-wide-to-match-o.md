---
id: TASK-145
title: 'sbt idea: rename group to outline app wide to match o'
status: To Do
assignee: []
created_date: '2026-09-03 21:39'
labels:
  - tui
  - idea
dependencies: []
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Filed from sbt 0.4.0 while at view=custom filter="" sort= selected=TASK-86 pane=None.

Impact: rename group to outline app wide to match o
Evidence: screen and action trail below, captured at filing time.

## Screen

```text
┌ switchbard  custom · hide:done · paint:1 · group:project · 49/107 ────────────────────────────────────────────────────────┐
│1 id 2 status    3 pri 4 title                                                                                             │
│80   To Do       H     Make the Task Queue aware of GitHub delivery state                                                  │
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
│144  To Do       M     sbt idea: refactor keyboard shortcuts                                                               │
│111  To Do       L     Goal page history card recomputes statuses per week per frame                                       │
│112  To Do       L     Digest sections disagree on repo scope when a single repo is drilled in                             │
│114  To Do       L     Perf-doc staleness sweep from IA V2                                                                 │
│86   Icebox      M     On Agent Context tab, enable quick actions like delete for commands, hooks, skills, etc             │
│                                                                                                                           │
│                                                                                                                           │
│                                                                                                                           │
│                                                                                                                           │
│                                                                                                                           │
│                                                                                                                           │
│                                                                                                                           │
│                                                                                                                           │
└───────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────┘
:idea rename group to outline app wide to match o▏
```

## Action trail

```text
session_start 0.4.0
config_reload 0
action settings
action settings (0.0ms)
action command (0.0ms)
report Idea TASK-144
action command idea (173.4ms)
action settings
action settings (0.1ms)
action settings
action settings_hide Done
action settings_promote
action settings
action settings_promote
action settings
action settings_promote
action settings
action command (0.0ms)
```
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Reporter confirms the behaviour in sbt matches what they were trying to do
<!-- AC:END -->
