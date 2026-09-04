---
id: TASK-158
title: Support named Ball holders in sbt
status: In Progress
assignee: []
created_date: '2026-09-04 15:01'
updated_date: '2026-09-04 15:13'
labels:
  - ball:agent
dependencies: []
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Extend the Ball field so a task can be held by me, an agent, or a named person. Preserve the fast b-key cycle for me/agent/none while allowing named ownership through the task CLI and rendering it in sbt.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 sb edit accepts a named holder such as Nick and stores one canonical ball:nick label
- [x] #2 sbt renders and filters a named holder in the Ball column
- [x] #3 Changing or clearing the Ball removes prior ball:* labels without disturbing unrelated labels
- [x] #4 Core, CLI, and sbt regression coverage passes
<!-- AC:END -->
