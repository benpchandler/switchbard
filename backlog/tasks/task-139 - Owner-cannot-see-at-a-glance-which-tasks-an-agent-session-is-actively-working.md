---
id: TASK-139
title: Owner cannot see at a glance which tasks an agent session is actively working
status: To Do
assignee: []
created_date: '2026-09-03 00:35'
labels:
  - gui
  - ux
  - product
dependencies: []
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Impact: with several agent sessions running against one repo, the owner opens the board and cannot tell which In Progress rows are being worked right now, by whom, versus rows that were started days ago and abandoned. On 2026-09-03 the board showed 25 In Progress rows while one session was actively on three of them; the owner had to ask in chat which ones. Wasted attention every time the owner looks, and stale In Progress rows become indistinguishable from live ones.

Problem statement: the data model has an assignee field, but nothing makes it the answer to 'who is on this now'. Agents rarely set it, it is free text with no convention tying it to a live session, it is not in the default board columns, and there is no signal that the claimant is still alive versus a session that ended hours ago. 'In Progress' is a work status, not a claim.

Evidence: owner's words 2026-09-03: 'I think we need a way for you to claim those tasks so I can see at a glance what you're working on.' Screenshot of the budget board with cols id,status,priority,title,labels and 25 In Progress rows. Interim: this session set assignee to claude:budget-75 on its three active tasks by hand. Related: TASK-137 (who is holding the task: owner vs agent vs external).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Opening the board, the owner can see which tasks are claimed by a live agent session, and by which one, without adding columns or filtering
- [ ] #2 A claim made by an agent session is distinguishable from a stale claim whose session has ended
- [ ] #3 Agents claim a task through one obvious sb command at the moment they start it, and the claim clears when they finish or release it
<!-- AC:END -->
