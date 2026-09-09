---
id: TASK-177
title: 'sbt idea: better format 2nd row header info (status!done row)'
status: To Do
assignee: []
created_date: '2026-09-08 11:36'
labels:
  - tui
  - idea
dependencies: []
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Filed from sbt 0.4.0 while at page=Tasks view=v1 filter="status:!done" sort= selected=TASK-176 pane=None.

Impact: better format 2nd row header info (status!done row)
Evidence: screen and action trail below, captured at filing time.

## Screen

```text
 [Tasks]   Pull Requests   tab switch page
┌ switchbard  v1 · status:!done · hide:done · group:project · 75/139 ────────────────────────────────────┐
│1 id  2 status    3 pri 4 title                                                                         │
│154   To Do       M     sbt: view history you can recognize - captured automatically, promoted to a slot│
│▸ no project                                                                                            │
│158   In Progress M     Support named Ball holders in sbt                                               │
│80.3  To Do       H     Build the Task Queue GitHub delivery backend                                    │
│80.4  To Do       H     Render mixed local and GitHub-backed work in the Task Queue                     │
│80.1  To Do       H     Prove the Task Queue with the Lucella delivery ledger                           │
│39    To Do       M     Reap dispatch runs orphaned by an app restart                                   │
│81    To Do       M     RemovalAuthorization: make the force gate a domain type, not a caller-supplied b│
│38    To Do       M     Unify List/Milestones row selection with Board's stroke-based indicator         │
│13    To Do       L     Virtualize Backlog task list rows for large repo/task counts                    │
│61    To Do       L     Landing worker: gh probe has no subprocess timeout                              │
│31    To Do       L     Tombstone filename collides on same-second consecutive wipes                    │
│36    To Do       L     Remove-repo confirmation can silently retarget between surfaces                 │
│68    To Do       L     Format fork: diverge on named wins                                              │
│87    To Do       H     Digest Tab: Clickable tasks                                                     │
│110   To Do       H     Goal check-in drafts survive week rollover with stale values                    │
│137   To Do       H     Owner cannot discover what is waiting on them without being told in chat        │
│139   To Do       H     Owner cannot see at a glance which tasks an agent session is actively working   │
│93    To Do       M     Give SB ability to detect refactoring candidates                                │
│94    To Do       M     Enable "integrations' vs hardcoded / config.                                    │
│95    To Do       M     sb add <title>: quick capture that falls back to the hub repo outside a Backlog │
│104   To Do       M     Create sprint from tasks / goals / projects                                     │
│108   To Do       M     TASK-56's cross-thread repaint race recurs in other backlog_controls.rs tests   │
│109   To Do       M     Retire the unreachable legacy Backlog lenses                                    │
│113   To Do       M     Support-request store for Command (NEEDS_DECISION/SITREP)                       │
│136   To Do       M     sbt: prior text on screen shows after terminal app quit and reload              │
│140   To Do       M     Id column truncation makes distinct ids look identical; the repeated repo prefix│
│143   To Do       M     sbt idea: when an idea comes in just start building it                          │
│144   To Do       M     sbt idea: refactor keyboard shortcuts                                           │
│145   To Do       M     sbt idea: rename group to outline app wide to match o                           │
│146   To Do       M     sbt idea: make evidence visisble                                                │
│147   To Do       M     sbt idea: switchbard persists all data rather than in repo; maybe writes some ki│
│148   To Do       M     sbt idea: when ranking a task, keep the cursor where the task was rather than mo│
│149   To Do       M     sbt idea: add a PR column ties to the tasks so u can see what's going on with th│
│151   To Do       M     sbt idea: migrate agents to graph                                               │
│155   To Do       M     sbt bug: in progress row 3 fading to brown / flashing brown during loop         │
│156   To Do       M     sbt idea: improve onboarding                                                    │
│169   To Do       M     Sent review feedback does not resume Codex after its turn ends                  │
│171   To Do       M     sbt bug: on PR 481, the app incorrectly reported that the PR wasn't mergeable ev│
│172   To Do       M     sbt bug: can't see PR tab in musicproduction repo on sbt load?                  │
│173   To Do       M     sbt bug: when build gets updated, active view gets reset                        │
│174   To Do       M     sbt idea: create the full task interaction menu under the t key -- create, edit,│
│175   To Do       M     sbt idea: enhance the formatting of the column headers to better differentiate f│
│176   To Do       M     sbt idea: enhance the formatting of the tab bar to better differntiate from belo│
└────────────────────────────────────────────────────────────────────────────────────────────────────────┘
:idea better format 2nd row header info (status!done row)▏
```

## Action trail

```text
session_start 0.4.0
action command (0.0ms)
report Idea TASK-175
action command idea (331.3ms)
action command (0.0ms)
report Idea TASK-176
action command idea (304.5ms)
action command (0.0ms)
```
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Reporter confirms the behaviour in sbt matches what they were trying to do
<!-- AC:END -->
