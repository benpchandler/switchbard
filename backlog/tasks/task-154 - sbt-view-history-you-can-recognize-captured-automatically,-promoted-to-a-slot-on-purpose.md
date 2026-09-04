---
id: TASK-154
title: 'sbt: view history you can recognize - captured automatically, promoted to a slot on purpose'
status: To Do
assignee: []
created_date: '2026-09-04 12:31'
updated_date: '2026-09-04 12:32'
labels:
  - tui
  - ux
dependencies:
  - TASK-152
  - TASK-153
priority: medium
project: Views That Keep Themselves
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Impact: every sbt user. The named-slot model asks for foresight - to keep an arrangement you must know while building it that you will want it back. Telemetry shows that is not how it is used: 355 view-mutating actions produced 2 saves in 45h. The owner's stated need is recognition, not recall: 'history is if we liked a certain view and wanted to get back to it but we didn't remember what it was.' History inverts the slot model - capture every distinct arrangement automatically, let the user recognize one later, then promote it to a real slot once they know they want it.

Evidence: ~/.switchbard/tui-events.jsonl, 2026-09-02..04. 135 sessions, 43 (31%) changed the view, 92 (68%) were navigation only. 3.0 session-ends per hour. Discovered 2026-09-04 alongside TASK-152 and TASK-153.

Two findings that shape the design, both from that data:
- Dedupe is load-bearing, not an optimization. Pushed on every quit, a 10-entry ring holds 3.3h of history because 68% of entries would duplicate their neighbour. Pushed only when the state differs, the same 10 entries hold 10.4h. Dedupe globally with move-to-front rather than only against the previous entry: the user returns to a handful of configurations, so the list collapses to 'distinct views actually used, most recent first' and self-limits.
- Legibility is the actual problem, not storage. Ten rows stamped with times are useless if each must be restored to see what it was. view_label (TASK-130) already renders a view in words and should carry the row, e.g. 'status:!done - by project - painted by status - 4 cols - 2d ago'.

Depends on TASK-152: entries that store resolved hex would drag a stale palette back onto the screen when restored, which is the TASK-152 bug multiplied by the number of entries and much harder to trace. Land tokens first.

Should be one mechanism, not two: an append-on-change history with two triggers (on quit, and on a timer during active use), not a separate quit-ring and snapshot system. Two paths writing the same shape is the one-fact-two-sources problem TASK-130 was written to remove.

Decision left open: the cap. Count-based is workable now that global dedupe collapses repeats, but the owner's use case ('we didn't remember what it was') implies days, and at ~23 view-changing sessions per day a count of 10 covers well under a day. Recommend capping by age (30d) with a generous count ceiling, and confirming the number with the owner.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Distinct view states are captured automatically with no user action, on quit and on a timer during active use, through one code path
- [ ] #2 Capture is deduped globally with move-to-front, so returning to a previous arrangement re-dates its entry instead of adding a duplicate
- [ ] #3 Each entry is listed with a human-readable view_label plus a relative time, enough to pick one without restoring it first
- [ ] #4 Selecting an entry restores that view, and there is a documented way to promote it into a numbered slot
- [ ] #5 Restoring a stored entry never reintroduces colors from a palette that is no longer selected (depends on TASK-152)
- [ ] #6 History is capped and the cap is documented; the capture file cannot grow without bound
<!-- AC:END -->
