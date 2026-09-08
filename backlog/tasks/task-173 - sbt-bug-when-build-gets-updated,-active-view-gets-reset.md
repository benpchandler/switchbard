---
id: TASK-173
title: 'sbt bug: when build gets updated, active view gets reset'
status: Done
assignee: []
created_date: '2026-09-08 11:30'
updated_date: '2026-09-08 12:40'
labels:
  - tui
  - bug
dependencies: []
priority: medium
project: Bugs
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Filed from sbt 0.4.0 while at view=v1 filter="status:!done" sort= selected=TASK-141.4 pane=None.

Impact: when build gets updated, active view gets reset
Evidence: screen and action trail below, captured at filing time.

## Screen

```text
┌ switchbard  v1 · status:!done · hide:done · group:project · 71/135 ────────────────────────────────────┐
│1 id  2 status    3 pri 4 title                                                                         │
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
│111   To Do       L     Goal page history card recomputes statuses per week per frame                   │
│112   To Do       L     Digest sections disagree on repo scope when a single repo is drilled in         │
│114   To Do       L     Perf-doc staleness sweep from IA V2                                             │
│86    Icebox      M     On Agent Context tab, enable quick actions like delete for commands, hooks, skil│
│141.1 In Review   M     Share PR column controls and saved view settings with Tasks                     │
│141.2 In Review   M     Bring PR filters to parity with Tasks                                           │
│141.3 In Review   M     Bring PR sorting to parity with Tasks                                           │
│141.4 In Review   M     Bring PR paint to parity with Tasks                                             │
│                                                                                                        │
│                                                                                                        │
└────────────────────────────────────────────────────────────────────────────────────────────────────────┘
:bug when build gets updated, active view gets reset▏
```

## Action trail

```text
session_start 0.4.0
action new_task (0.0ms)
action new_task_cancel
action new_task (0.0ms)
action new_task_cancel
action task (0.0ms)
unbound W
unbound h
action command (0.0ms)
```
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Reporter confirms the behaviour in sbt matches what they were trying to do
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Cause found in the self-restart handoff. When `cargo install` replaces the binary, every running sbt re-execs and hands its live view to the new build through SBT_RESUME. That record was a positional JSON tuple, and both ends of the handoff are different builds. resume_pages() ended with 'let Ok(...) = parsed else { return; }' - a record the arriving build could not parse was dropped in silence, leaving the app on whatever App::open had already loaded, which is saved slot 1. For this repo slot 1 is 'status:!done, group:project', so a custom view came back looking like a plausible v1 with no sign anything was lost.

The tuple had already been broken once and patched: resume_pages carried a hand-written fallback for the 7-field arity after an 8th field was added. That is the same failure in the other direction.

Fix: the record moved to crates/switchbard-tui/src/app/resume.rs as a named-field object with a format prefix. A build that gained a field still reads an older record (missing fields default), a build that lost one still reads a newer record (unknown fields ignored), and a record we genuinely cannot read now sets 'the new build could not read the previous view; opened your saved view' instead of pretending nothing happened. The old positional formats are still read on the way in.

Evidence: tests/browse.rs::a_view_survives_a_restart_into_a_build_with_a_field_this_one_lacks and ::a_resume_record_this_build_cannot_read_is_reported_not_swallowed, plus five unit tests in app/resume.rs covering both legacy arities, extra fields, missing fields and cold start. Both browse tests verified failing against the old implementation.

Related, not fixed here: TASK-153 (resume the last view on any launch, not just across self-restart) would make this recoverable rather than merely loud.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
A self-restart hands the live view to a different build, and the handoff was a positional tuple that the arriving build silently discarded when it could not parse it - dropping the user back on saved slot 1 with no message. The record is now a named, versioned, field-tolerant object in its own module, and an unreadable one is reported rather than swallowed. Fixed on fix/tui-bug-sweep (370c2d1).
<!-- SECTION:FINAL_SUMMARY:END -->
