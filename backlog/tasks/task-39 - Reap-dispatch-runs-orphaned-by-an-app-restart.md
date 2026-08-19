---
id: TASK-39
title: Reap dispatch runs orphaned by an app restart
status: To Do
assignee: []
created_date: '2026-08-07 01:28'
labels: []
dependencies: []
ordinal: 39000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
A task claimed by dispatch_one is stranded on the 'dispatching' label forever if Switchbard exits mid-run: the headless claude process is a session leader and survives, finishes, and writes its log, but finish_pipeline (push + gh pr create) and release_as_dispatched/release_as_failed die with the parent. Observed live on MusicProduction TASK-307 (2026-08-06): agent committed a9e90327, branch never pushed, no PR, task still labeled 'dispatching'.

The queue guard is intentionally one-way ('never back on dispatch, so a failed run doesn't silently retry-loop'), but that assumes the pipeline lived long enough to release the claim.

dispatch_inspect already supplies the evidence needed to detect this: elapsed far past DispatchOptions::timeout AND log_has_output() true (the agent finished) AND the label still 'dispatching' means an orphan, not a live run. The Dispatch view surfaces it; nothing acts on it.

Consider: a reaper pass on startup that marks such tasks dispatch-failed with a note, and/or a per-row 'resume' action in the Dispatch view that runs the remaining push + PR steps against the existing worktree commit.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 An orphaned dispatching task is detected and does not stay in-flight forever
- [ ] #2 The agent's committed work is never discarded by the recovery path
<!-- AC:END -->
