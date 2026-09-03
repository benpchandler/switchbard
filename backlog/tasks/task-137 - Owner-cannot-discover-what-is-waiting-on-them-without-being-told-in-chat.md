---
id: TASK-137
title: Owner cannot discover what is waiting on them without being told in chat
status: To Do
assignee: []
created_date: '2026-09-02 22:43'
labels:
  - gui
  - ux
  - product
dependencies: []
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Impact: the owner is the single human in a multi-agent workflow, and agents regularly reach a point only the owner can clear: run a login command, hand over a credential, choose between named options, perform a console step. Today that ask exists only in the chat transcript of whichever session raised it. The owner has to be watching that session at that moment, or be told again later, to learn it exists. Work stalls silently for as long as the owner is elsewhere; on 2026-09-03 a staging proof sat blocked on a two-step owner action while the board showed the task as an ordinary In Progress row among 25 others. Missed or late asks also produce the reverse failure: a goal was checked in as met because nobody could see the owner-side step had never been asked or answered.

Problem statement: the board has no notion of who is holding a task right now. Status (To Do / In Progress) describes the work, not whether progress depends on the owner, an agent, or an outside party. A label or note is opt-in and pull-only: the owner has to know to filter for it. There is no way for the owner to open Switchbard and answer, in one glance, 'what is waiting on me, since when, and what exactly do I have to do'. There is also no channel that pushes that fact to the owner when they are not looking at the board.

Evidence: budget repo session 2026-09-03 (LED-639.3, LED-639.2, LED-649.1 carried owner-only steps visible only in chat; interim workaround was a needs-owner label plus a NEEDS OWNER note, which the owner correctly called undiscoverable). Owner's words: 'how is that discoverable for me unless you tell me? that's my push. what does the screen need?'
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Opening Switchbard, the owner can see every task currently waiting on them in one place, without filtering or being told, with the exact ask and how long it has waited
- [ ] #2 When an agent hands a task to the owner, the owner learns about it without looking at the board (push, not pull)
- [ ] #3 When the owner acts or decides, the answer is recorded on the task so a later session does not re-ask, and the task visibly stops waiting on the owner
- [ ] #4 A task waiting on the owner is distinguishable from one an agent is actively working, and from one blocked on an external party, in the default board view
<!-- AC:END -->
