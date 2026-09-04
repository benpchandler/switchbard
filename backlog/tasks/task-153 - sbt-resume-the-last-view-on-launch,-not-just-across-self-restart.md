---
id: TASK-153
title: 'sbt: resume the last view on launch, not just across self-restart'
status: To Do
assignee: []
created_date: '2026-09-04 12:31'
updated_date: '2026-09-04 12:32'
labels:
  - tui
  - ux
dependencies: []
priority: high
project: Views That Keep Themselves
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Impact: every sbt user. Filter, sort, columns, paint, group and rank are lost on every exit unless the user thinks to press vs<n> first, which requires knowing at the time that the arrangement will be wanted later. Telemetry over 45h shows the save step is simply not in the loop: 355 view-mutating actions (paint 115, column 82, filter 76, group 40, sort 23, rank 13, glyph 6) produced 2 view saves - 177 mutations per save - across 207 session ends of which only 6 were an explicit q. Every one of those ends silently discarded whatever was on screen.

Evidence: ~/.switchbard/tui-events.jsonl, window 2026-09-02..2026-09-04, 135 session_start / 207 session_end / 199 self_restart. Counted with a jq-equivalent pass over kind=action details. Discovered 2026-09-04 while investigating why a stale paint rule survived a theme change.

The mechanism already exists and is not wired to launch: TASK-130 (Done) built resume_state using the same serialization as views.lua, and it fires on all 199 self_restart events. This task is to read that record on a cold start too.

Must not fold into the default slot: vsd is a deliberate act meaning 'this is my home view'. If quitting overwrote it, the user loses the ability to have a stable home at all and would not see it happen. Resume state stays its own record; the default slot stays deliberate.

Decision left open: whether a cold start always resumes, or resumes only when the previous session ended cleanly, and whether a flag (--fresh) opts out. Recommend always resume plus --fresh, since 68% of sessions never change the view and would resume an identical state anyway.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A cold sbt start restores the view state the previous session ended with, per repo
- [ ] #2 Resume uses the same record and serialization as self_restart resume_state - no second persistence path is added
- [ ] #3 The default slot (vsd/vgd) is never written by exit or launch; it changes only when the user saves it
- [ ] #4 A first-ever run in a repo with no resume record opens the default slot exactly as it does today
- [ ] #5 There is a documented way to start fresh without resuming
<!-- AC:END -->
