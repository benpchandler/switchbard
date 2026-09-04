---
id: TASK-150
title: 'Live work marker: sbt blinks the rows an agent session is working; sb work claim/release/pass; harness hooks enforce claim-before-edit and no-stop-while-claimed'
status: In Progress
assignee: []
created_date: '2026-09-04 10:22'
updated_date: '2026-09-04 10:33'
labels:
  - tui
  - agent-protocol
  - ball:agent
dependencies: []
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Owner request 2026-09-04: reflect which task a session is working on the sbt screen (multiple), as an enforceable harness hook so an agent has to grab a task and keep working until a human passes it or its ACs are all met.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 sbt shows a blinking work marker on every task a live agent session has claimed (several sessions, several tasks)
- [ ] #2 sb work claim/release/pass/list/hook write only through the core work-session store; a dead session's claim disappears without a human step
- [ ] #3 PreToolUse hook denies Edit/Write until the session has claimed a task in the repo; Stop hook blocks stopping while a claim is held, bounded so it cannot loop forever
- [ ] #4 release requires every AC checked or an explicit --note; pass (human) releases regardless
- [ ] #5 E2E tests cover live, dead, multiple sessions, pass from sbt, and the hook decisions
<!-- AC:END -->
