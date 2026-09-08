---
id: TASK-180
title: 'sbt bug: ensure the CI checks do not run the full rust check on every PR, only where needed'
status: Done
assignee: []
created_date: '2026-09-08 12:42'
updated_date: '2026-09-08 13:23'
labels:
  - ci
  - build
  - bug
dependencies: []
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Filed from sbt 0.4.0 while at page=Tasks view=custom filter="status:!done" sort= selected=TASK-80 pane=None.

Impact: ensure the CI checks do not run the full rust check on every PR, only where needed
Evidence: screen and action trail below, captured at filing time.

## Screen

```text
 [Tasks]   Pull Requests   tab switch page
PR #139: state: Merged                                                                       1 · n dismiss
┌ switchbard  custom · status:!done · cols:id,status,priority,title,rank · hide:done · group:project · 73┐
│1 id  2 status    3 pri 4 title                                                                      5 #│
│▸ top · 2                                                                                               │
│141   To Do       M     sbt idea: PR page with actions and status notifications across pages         1  │
│80    To Do       H     Make the Task Queue aware of GitHub delivery state                           2  │
│▸ EGUI Polish · Planned · 0/2                                                                           │
│78    To Do       H     Elevation scale tokens in theme.rs                                              │
│79    To Do       M     Sweep surfaces onto the elevation scale                                         │
│▸ Abstraction · Planned · 0/6                                                                           │
│162   To Do       M     Extract reusable column definitions and sorting                                 │
│163   To Do       M     Decouple filtering from task entities                                           │
│164   To Do       M     Extract reusable paint rule evaluation                                          │
│165   To Do       M     Make shared list UI components independent of whole-app state                   │
│166   To Do       M     Generalize saved views across list features                                     │
│167   To Do       M     Paint rows by when a PR merged and when a task was filed                        │
│▸ GitHub Operations · Planned · 0/6                                                                     │
│115   To Do       H     Lock the GitHub Operations authority, placement, and command contract           │
│116   To Do       H     Extend GitHub delivery observations for repository pull-request operations      │
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
└────────────────────────────────────────────────────────────────────────────────────────────────────────┘
:bug ensure the CI checks do not run the full rust check on every PR, only where needed▏
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
```
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Reporter confirms the behaviour in sbt matches what they were trying to do
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Reproduced from the record. CI run 34250656057 on fix/pr-138-task-references ran fmt (23s), clippy on both runners (62s + 39s) and test on both (184s + 200s) - and went red on a GUI test - for a PR whose entire diff was three files:
  backlog/tasks/task-119 ...md
  backlog/tasks/task-141 ...md
  backlog/tasks/task-141.5 ...md
Backlog markdown cannot change a Rust build's result, so ~8 minutes of runner time and a red gate came from nothing the change touched. (This is also what surfaced TASK-181.)

The mission-sidecar matrix was already routed by a change-scope job; the Rust matrix was not.

Fix: scripts/ci-change-scope.sh replaces scripts/ci-mission-sidecar-diff.sh as the one place that resolves the diff, and hands the same path list to one scope script per matrix - so adding a third scope needs no second diff implementation. scripts/ci-rust-scope.sh answers for the Rust matrix and returns false only when EVERY changed path is a backlog record (backlog/**), the docs tree (docs/**), or root-level Markdown. Everything else - crates, scripts, workflows, toolchain pins, lockfiles, assets, nested .md - runs it.

Fails open by construction: an unresolvable base, a diff that cannot be taken, an empty diff, or any unclassified path runs everything. The default is 'run it' because a false green on a merge is unbounded while a needless matrix run costs minutes.

Evidence:
- scripts/test-ci-rust-scope.sh pins the verdicts, including PR #139's actual three-file diff -> false, and the fail-open cases -> true.
- scripts/test-ci-change-scope.sh drives the real Git plumbing in a scratch repo and asserts a backlog-only commit routes rust=false, mission_sidecar=false.
- scripts/test-ci-workflow.rb now fails if the ci job stops depending on change-scope or stops reading its rust output; verified failing when the gate is removed ('Rust matrix routing condition lost: nil').
- All of the above run in the change-scope job on every PR (9s), so the router is itself gated.

Note: main has no branch protection (checked: 'Branch not protected'), so a skipped job cannot block a merge on a required check. If protection is added later, add a small always-run summary job rather than making these matrices unconditional again.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Every PR ran the full Rust matrix on both runners regardless of what it touched - PR #139, three backlog markdown files, burned ~8 minutes and went red on an unrelated GUI test. The Rust matrix is now routed the same way the mission-sidecar matrix already was, through one change-scope script that diffs once and asks a scope script per matrix. It skips only when every changed path is a backlog record or prose, fails open on anything else, and the routing is itself guarded by three gate tests that run on every PR. Fixed on fix/tui-bug-sweep (9a68415).
<!-- SECTION:FINAL_SUMMARY:END -->
