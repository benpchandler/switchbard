---
id: TASK-155
title: 'sbt bug: in progress row 3 fading to brown / flashing brown during loop'
status: To Do
assignee: []
created_date: '2026-09-04 12:31'
labels:
  - tui
  - bug
dependencies: []
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Filed from sbt 0.4.0 while at view=custom filter="" sort= selected=TASK-150 pane=None.

Impact: in progress row 3 fading to brown / flashing brown during loop
Evidence: screen and action trail below, captured at filing time.

## Screen

```text
┌ switchbard  custom · cols:id,status,priority,title,rank · hide:done · group:project · working:1 · 56/117 ────┐
│1 id 2 status    3 pri 4 title                                                                             5 #│
│▸ top · 3                                                                                                     │
│80   To Do       H     Make the Task Queue aware of GitHub delivery state                                  1  │
│141  To Do       M     sbt idea: PR page with actions and status notifications across pages                2  │
│150  In Progress H     Live work marker: sbt blinks the rows an agent session is working; sb work claim/re 3  │
│▸ EGUI Polish · Planned · 0/2                                                                                 │
│78   To Do       H     Elevation scale tokens in theme.rs                                                     │
│79   To Do       M     Sweep surfaces onto the elevation scale                                                │
│▸ GitHub Operations · Planned · 0/6                                                                           │
│115  To Do       H     Lock the GitHub Operations authority, placement, and command contract                  │
│116  To Do       H     Extend GitHub delivery observations for repository pull-request operations             │
│117  To Do       H     Build the canonical Ops > Pull requests surface                                        │
│118  To Do       H     Add guarded pull-request review operations                                             │
│119  To Do       H     Add guarded CI, branch-update, and merge operations                                    │
│120  To Do       H     Dogfood and release GitHub Operations in the native app                                │
│▸ Instant Cold Start · Planned · 0/6                                                                          │
│121  To Do       H     Lock the instant-startup contract and failing first-frame journey                      │
│122  To Do       H     Build the bounded sharded startup snapshot kernel                                      │
│123  To Do       H     Render Servers immediately from last-known topology and processes                      │
│124  To Do       H     Render Workspace immediately from last-known Git and worktree state                    │
│125  To Do       H     Render Tasks and Dispatch immediately from last-known read models                      │
│126  To Do       M     Unify Agents caching and close the cross-platform startup gates                        │
│▸ no project                                                                                                  │
│80.3 To Do       H     Build the Task Queue GitHub delivery backend                                           │
│80.4 To Do       H     Render mixed local and GitHub-backed work in the Task Queue                            │
│80.1 To Do       H     Prove the Task Queue with the Lucella delivery ledger                                  │
│39   To Do       M     Reap dispatch runs orphaned by an app restart                                          │
│81   To Do       M     RemovalAuthorization: make the force gate a domain type, not a caller-supplied bool    │
│38   To Do       M     Unify List/Milestones row selection with Board's stroke-based indicator                │
│13   To Do       L     Virtualize Backlog task list rows for large repo/task counts                           │
│61   To Do       L     Landing worker: gh probe has no subprocess timeout                                     │
│31   To Do       L     Tombstone filename collides on same-second consecutive wipes                           │
│36   To Do       L     Remove-repo confirmation can silently retarget between surfaces                        │
│68   To Do       L     Format fork: diverge on named wins                                                     │
│87   To Do       H     Digest Tab: Clickable tasks                                                            │
│110  To Do       H     Goal check-in drafts survive week rollover with stale values                           │
│137  To Do       H     Owner cannot discover what is waiting on them without being told in chat               │
│139  To Do       H     Owner cannot see at a glance which tasks an agent session is actively working          │
│152  To Do       H     sbt: paint_auto stores a palette token, not a resolved hex                             │
│153  To Do       H     sbt: resume the last view on launch, not just across self-restart                      │
│93   To Do       M     Give SB ability to detect refactoring candidates                                       │
│94   To Do       M     Enable "integrations' vs hardcoded / config.                                           │
│95   To Do       M     sb add <title>: quick capture that falls back to the hub repo outside a Backlog rep    │
│104  To Do       M     Create sprint from tasks / goals / projects                                            │
│108  To Do       M     TASK-56's cross-thread repaint race recurs in other backlog_controls.rs tests          │
│109  To Do       M     Retire the unreachable legacy Backlog lenses                                           │
│113  To Do       M     Support-request store for Command (NEEDS_DECISION/SITREP)                              │
│136  To Do       M     sbt: prior text on screen shows after terminal app quit and reload                     │
│140  To Do       M     Id column truncation makes distinct ids look identical; the repeated repo prefix wa    │
│143  To Do       M     sbt idea: when an idea comes in just start building it                                 │
└──────────────────────────────────────────────────────────────────────────────────────────────────────────────┘
:bug in progress row 3 fading to brown / flashing brown during loop▏
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
action up (0.0ms)
config_reload 0
action reload (13.9ms)
action task (0.0ms)
action down (0.0ms)
action down (0.0ms)
action up (0.0ms)
action up (0.0ms)
action task (0.0ms)
action rank 3 TASK-150
unbound h
unbound i
action sort_column (0.0ms)
config_reload 0
action reload (5.9ms)
action paint
action paint (0.0ms)
action paint_clear_all
action paint
action paint (0.0ms)
action paint_clear_all
unbound backspace
action command (0.0ms)
```
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Reporter confirms the behaviour in sbt matches what they were trying to do
<!-- AC:END -->
