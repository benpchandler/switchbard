---
id: TASK-186
title: 'sbt idea: enable subtask linking in same menu as parent task linking'
status: To Do
assignee: []
created_date: '2026-09-08 14:17'
labels:
  - tui
  - idea
dependencies: []
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Filed from sbt 0.4.0 while at page=Tasks view=custom filter="status:!done" sort=id:ascending selected=TASK-185 pane=None.

Impact: enable subtask linking in same menu as parent task linking
Evidence: screen and action trail below, captured at filing time.

## Screen

```text
 [Tasks]   Pull Requests   tab switch page
┌ switchbard  custom · status:!done · ↑id · cols:id,status,priority,title,rank,ball · hide:done · group:p┐
│1 id  2 status    3 pri 4 title                                                               5 # 6 ball│
│80.1  To Do       H     Prove the Task Queue with the Lucella delivery ledger                           │
│80.3  To Do       H     Build the Task Queue GitHub delivery backend                                    │
│80.4  To Do       H     Render mixed local and GitHub-backed work in the Task Queue                     │
│81    To Do       M     RemovalAuthorization: make the force gate a domain type, not a caller           │
│86    Icebox      M     On Agent Context tab, enable quick actions like delete for commands,            │
│87    To Do       H     Digest Tab: Clickable tasks                                                     │
│93    To Do       M     Give SB ability to detect refactoring candidates                                │
│94    To Do       M     Enable "integrations' vs hardcoded / config.                                    │
│95    To Do       M     sb add <title>: quick capture that falls back to the hub repo outside           │
│104   To Do       M     Create sprint from tasks / goals / projects                                     │
│109   To Do       M     Retire the unreachable legacy Backlog lenses                                    │
│110   To Do       H     Goal check-in drafts survive week rollover with stale values                    │
│111   To Do       L     Goal page history card recomputes statuses per week per frame                   │
│112   To Do       L     Digest sections disagree on repo scope when a single repo is drilled            │
│113   To Do       M     Support-request store for Command (NEEDS_DECISION/SITREP)                       │
│114   To Do       L     Perf-doc staleness sweep from IA V2                                             │
│136   To Do       M     sbt: prior text on screen shows after terminal app quit and reload              │
│137   To Do       H     Owner cannot discover what is waiting on them without being told in c           │
│139   To Do       H     Owner cannot see at a glance which tasks an agent session is actively           │
│140   To Do       M     Id column truncation makes distinct ids look identical; the repeated            │
│141.1 In Review   M     Share PR column controls and saved view settings with Tasks               me    │
│141.2 In Review   M     Bring PR filters to parity with Tasks                                     me    │
│141.3 In Review   M     Bring PR sorting to parity with Tasks                                     me    │
│141.4 In Review   M     Bring PR paint to parity with Tasks                                       me    │
│143   To Do       M     sbt idea: when an idea comes in just start building it                          │
│144   To Do       M     sbt idea: refactor keyboard shortcuts                                           │
│145   To Do       M     sbt idea: rename group to outline app wide to match o                           │
│146   To Do       M     sbt idea: make evidence visisble                                                │
│147   To Do       M     sbt idea: switchbard persists all data rather than in repo; maybe wri           │
│148   To Do       M     sbt idea: when ranking a task, keep the cursor where the task was rat           │
│149   To Do       M     sbt idea: add a PR column ties to the tasks so u can see what's going           │
│151   To Do       M     sbt idea: migrate agents to graph                                               │
│156   To Do       M     sbt idea: improve onboarding                                                    │
│158   In Progress M     Support named Ball holders in sbt                                         agent │
│169   To Do       M     Sent review feedback does not resume Codex after its turn ends                  │
│174   To Do       M     sbt idea: create the full task interaction menu under the t key -- cr           │
│175   To Do       M     sbt idea: enhance the formatting of the column headers to better diff           │
│176   To Do       M     sbt idea: enhance the formatting of the tab bar to better differntiat           │
│177   To Do       M     sbt idea: better format 2nd row header info (status!done row)                   │
│179   To Do       H     Shared CARGO_TARGET_DIR lets one worktree's build answer another work           │
│182   In Review   M     Resolve and validate parent links through the shared task layer           me    │
│183   In Review   M     Pick an existing parent task by ID and title from the task menu           me    │
│184   To Do       M     sbt idea: bottom command / status bar should show last action taken a           │
│185   To Do       M     Preserve live work claims when reparenting changes a task ID                    │
└────────────────────────────────────────────────────────────────────────────────────────────────────────┘
:idea enable subtask linking in same menu as parent task linking▏
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
