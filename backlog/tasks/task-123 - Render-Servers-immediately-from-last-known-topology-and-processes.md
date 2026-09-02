---
id: TASK-123
title: Render Servers immediately from last-known topology and processes
status: To Do
assignee: []
created_date: '2026-09-01 17:44'
labels:
  - cold-start
  - servers
  - egui
  - performance
dependencies:
  - TASK-122
priority: high
project: Instant Cold Start
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Impact: Servers is the default view, so an empty first frame makes Switchbard appear broken even when the prior session already knew the repos, worktrees, listeners, and declared services.

Evidence: main.rs currently performs live worktree enumeration before window creation; ScanState and service maps start empty; the scanner refreshes first but service detection is deliberately staggered later.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Open the native window without running Git enumeration, lsof or proc scanning, filesystem tree walks, du, or network subprocesses on the pre-window path.
- [ ] #2 With valid cached topology, listeners, and detected services, render representative Servers content on the first frame while live workers are deliberately blocked.
- [ ] #3 Label cached process data as last seen and keep Kill, Stop, Open Port, and similar live-process actions disabled until a current process probe authenticates them.
- [ ] #4 Keep Start disabled for cached declarations until that worktree service detection refreshes successfully.
- [ ] #5 A successful live empty result replaces stale non-empty data; a failed refresh preserves useful cached data with explicit age and failure state.
- [ ] #6 Current config filters hydration: removed repos never reappear, new repos appear immediately as placeholders, and repo names or aliases come from current config.
<!-- AC:END -->
