---
id: TASK-113
title: Support-request store for Command (NEEDS_DECISION/SITREP)
status: To Do
assignee: []
created_date: '2026-09-01 08:20'
labels:
  - dispatch
  - command
  - core
dependencies: []
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
No core store exists for support requests; Command's support card is evidence-only (state/elapsed/log) by documented design (ui/places/command.rs ~33-58).

Impact: when an agent raises a NEEDS_DECISION or SITREP, its real question text cannot surface anywhere in the GUI - the owner must go open logs to find out what's actually being asked, slowing every decision response.

Evidence: ui/places/command.rs ~33-58 (module doc documenting the evidence-only design and the absence of a request-question surface).

Requires a design decision on the record's shape and writer - do not pre-build without it.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Design decision made and recorded on the support-request record's shape and its single writer
- [ ] #2 Core record type for support requests added behind that decision, one writer
- [ ] #3 Command renders the question text plus a respond affordance
- [ ] #4 Trajectory doc updated to reflect the new capability
<!-- AC:END -->
