---
id: TASK-150
title: 'Live work marker: sbt blinks the rows an agent session is working; sb work claim/release/pass; harness hooks enforce claim-before-edit and no-stop-while-claimed'
status: In Progress
assignee: []
created_date: '2026-09-04 10:22'
updated_date: '2026-09-04 10:41'
labels:
  - tui
  - agent-protocol
  - ball:agent
dependencies: []
priority: high
references:
  - https://github.com/benpchandler/switchbard/pull/134
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Owner request 2026-09-04: reflect which task a session is working on the sbt screen (multiple), as an enforceable harness hook so an agent has to grab a task and keep working until a human passes it or its ACs are all met.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 sbt shows a blinking work marker on every task a live agent session has claimed (several sessions, several tasks)
- [x] #2 sb work claim/release/pass/list/hook write only through the core work-session store; a dead session's claim disappears without a human step
- [x] #3 PreToolUse hook denies Edit/Write until the session has claimed a task in the repo; Stop hook blocks stopping while a claim is held, bounded so it cannot loop forever
- [x] #4 release requires every AC checked or an explicit --note; pass (human) releases regardless
- [x] #5 E2E tests cover live, dead, multiple sessions, pass from sbt, and the hook decisions
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
PR #134. Store: core work_sessions (~/.switchbard/work/<session>.json, live while CLAUDE_PID is). Protocol: sb work claim/release/pass/list/hook. Hooks: .claude/settings.json (PreToolUse edit tools, Stop, SessionEnd) -> sb work hook. sbt: work column, working surface blink (work.blink_ms), title working:N, detail lines, w = pass. Proof: tests/work.rs, cli.rs work_* tests, live hook run against the authoring session, preflight green. Left for owner: exit condition beyond all-ACs-or-note; 10th column puts the columns picker into two-digit mode for 1.
<!-- SECTION:NOTES:END -->
