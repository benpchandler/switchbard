---
id: TASK-181
title: 'sbt bug: An existing GUI cleanup test exceeded its four-frame repaint limit while background'
status: Done
assignee: []
created_date: '2026-09-08 12:43'
updated_date: '2026-09-08 13:24'
labels:
  - test-infra
  - flaky-test
  - bug
dependencies: []
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Filed from sbt 0.4.0 while at page=Tasks view=custom filter="status:!done" sort= selected=TASK-180 pane=None.

Impact: An existing GUI cleanup test exceeded its four-frame repaint limit while background
Evidence: screen and action trail below, captured at filing time.

## Screen

```text
 [Tasks]   Pull Requests   tab switch page
PR #139: state: Merged                                                                       1 · n dismiss
┌ switchbard  custom · status:!done · cols:id,status,priority,title,rank · hide:done · group:project · 74┐
│1 id  2 status    3 pri 4 title                                                                      5 #│
│154   To Do       M     sbt: view history you can recognize - captured automatically, promoted to a     │
│▸ no project                                                                                            │
│158   In Progress M     Support named Ball holders in sbt                                               │
│80.3  To Do       H     Build the Task Queue GitHub delivery backend                                    │
│80.4  To Do       H     Render mixed local and GitHub-backed work in the Task Queue                     │
│80.1  To Do       H     Prove the Task Queue with the Lucella delivery ledger                           │
│39    To Do       M     Reap dispatch runs orphaned by an app restart                                   │
│81    To Do       M     RemovalAuthorization: make the force gate a domain type, not a caller-suppli    │
│38    To Do       M     Unify List/Milestones row selection with Board's stroke-based indicator         │
│13    To Do       L     Virtualize Backlog task list rows for large repo/task counts                    │
│61    To Do       L     Landing worker: gh probe has no subprocess timeout                              │
│31    To Do       L     Tombstone filename collides on same-second consecutive wipes                    │
│36    To Do       L     Remove-repo confirmation can silently retarget between surfaces                 │
│68    To Do       L     Format fork: diverge on named wins                                              │
│87    To Do       H     Digest Tab: Clickable tasks                                                     │
│110   To Do       H     Goal check-in drafts survive week rollover with stale values                    │
│137   To Do       H     Owner cannot discover what is waiting on them without being told in chat        │
│139   To Do       H     Owner cannot see at a glance which tasks an agent session is actively workin    │
│179   To Do       H     Shared CARGO_TARGET_DIR lets one worktree's build answer another worktree's     │
│93    To Do       M     Give SB ability to detect refactoring candidates                                │
│94    To Do       M     Enable "integrations' vs hardcoded / config.                                    │
│95    To Do       M     sb add <title>: quick capture that falls back to the hub repo outside a Back    │
│104   To Do       M     Create sprint from tasks / goals / projects                                     │
│108   To Do       M     TASK-56's cross-thread repaint race recurs in other backlog_controls.rs test    │
│109   To Do       M     Retire the unreachable legacy Backlog lenses                                    │
│113   To Do       M     Support-request store for Command (NEEDS_DECISION/SITREP)                       │
│136   To Do       M     sbt: prior text on screen shows after terminal app quit and reload              │
│140   To Do       M     Id column truncation makes distinct ids look identical; the repeated repo pr    │
│143   To Do       M     sbt idea: when an idea comes in just start building it                          │
│144   To Do       M     sbt idea: refactor keyboard shortcuts                                           │
│145   To Do       M     sbt idea: rename group to outline app wide to match o                           │
│146   To Do       M     sbt idea: make evidence visisble                                                │
│147   To Do       M     sbt idea: switchbard persists all data rather than in repo; maybe writes som    │
│148   To Do       M     sbt idea: when ranking a task, keep the cursor where the task was rather tha    │
│149   To Do       M     sbt idea: add a PR column ties to the tasks so u can see what's going on wit    │
│151   To Do       M     sbt idea: migrate agents to graph                                               │
│156   To Do       M     sbt idea: improve onboarding                                                    │
│169   To Do       M     Sent review feedback does not resume Codex after its turn ends                  │
│174   To Do       M     sbt idea: create the full task interaction menu under the t key -- create, e    │
│175   To Do       M     sbt idea: enhance the formatting of the column headers to better differentia    │
│176   To Do       M     sbt idea: enhance the formatting of the tab bar to better differntiate from     │
│177   To Do       M     sbt idea: better format 2nd row header info (status!done row)                   │
│180   To Do       M     sbt bug: ensure the CI checks do not run the full rust check on every PR, on    │
└────────────────────────────────────────────────────────────────────────────────────────────────────────┘
:bug An existing GUI cleanup test exceeded its four-frame repaint limit while background▏
```

## Action trail

```text
session_start 0.4.0
action page (0.1ms)
action up (0.0ms)
action open (0.0ms)
action back (0.0ms)
action page (0.3ms)
action command (0.0ms)
report Bug TASK-180
action command bug (315.2ms)
unbound space
unbound backspace
action command (0.0ms)
```
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Reporter confirms the behaviour in sbt matches what they were trying to do
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Found the exact failure: CI run 34250656057, macos-latest / mise run test.

  crates/switchbard-gui/tests/qa_reverify_2026_08_05.rs:580
  clean_up_old_tasks_confirm_archives_the_done_task_in_both_real_repos
  Harness::run exceeded max_steps (4)
  Repaint causes: [ crates/switchbard-gui/src/app.rs:2127 ]

Line 580 is the h.run() immediately after clicking 'Confirm cleanup'. app.rs:2127 is the ctx.request_repaint() the cleanup worker issues once per task. Harness::run settles to quiescence, so it can never settle while that worker is alive - this is TASK-56's race, third occurrence.

Reproduced deterministically rather than by waiting for load: seeding 60 Done tasks per repo instead of one makes the worker outlast the step budget every time, and the test then failed locally with the identical panic, site and repaint cause. With the fix applied the same seeded run gets past it (failing only on the seeded count assertion, as expected).

Fix: the rule now has one home instead of being re-derived per test. crates/switchbard-gui/tests/common/mod.rs gains step_past_spawn() and settle_until(), documented with why run() cannot be used after a spawn. Applied at the reported site, at the identical twin in qa_reverify_2026_08_05_wave2.rs, and in place of three hand-rolled 'run_steps(4) + deadline poll' loops in backlog_controls.rs.

This also covers TASK-108, which is the same defect in backlog_controls.rs - see its notes.

Verification: mise run test six consecutive times, all green, zero failures. Full mise run ci green (85 test binaries).
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
A cleanup test called Harness::run right after the click that spawns the cleanup worker; the worker requests a repaint per task, so run() could never settle and tripped its four-step budget. Reproduced deterministically by seeding enough Done tasks, then fixed by giving the rule one home - step_past_spawn/settle_until in tests/common - and applying it at the reported site, its twin, and three hand-rolled equivalents. Six consecutive green test runs. Fixed on fix/tui-bug-sweep (9a68415).
<!-- SECTION:FINAL_SUMMARY:END -->
