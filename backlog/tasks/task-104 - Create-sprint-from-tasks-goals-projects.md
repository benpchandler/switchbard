---
id: TASK-104
title: Create sprint from tasks / goals / projects
status: To Do
assignee: []
created_date: '2026-09-01 02:25'
labels:
  - cli
  - gui
  - task-queue
dependencies: []
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Impact: owner assembles each sprint by hand across three surfaces (task list, goals.yml, project definitions); nothing composes them into one time-boxed unit, so weekly planning is manual re-derivation. Requested verbatim by the owner 2026-09-01 while setting the week's Lucella goals.

Evidence: goals (backlog/goals.yml), projects/initiatives (backlog/projects/, backlog/initiatives/), and ranked tasks all exist as separate sb families with no sprint construct joining them; this week's Lucella sprint was assembled by hand in conversation.

Decision needed: whether a sprint is a first-class record (backlog/sprints.yml linking a week to goal names + task ids + project scopes) or a derived view (sb sprint view composing the current week's goals with their scoped tasks). Record the choice before implementing.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 An sb verb creates or shows a sprint composed from existing tasks, goals, and/or projects for a given week
- [ ] #2 The sprint surface shows per-goal pace and member task done/total in one output
- [ ] #3 Design decision (record vs derived view) recorded in the task before implementation
<!-- AC:END -->
