---
id: TASK-83
title: 'CLI: rank / expedite verbs and create-time placement'
status: To Do
assignee: []
created_date: '2026-08-31 22:01'
labels:
  - backlog
  - cli
dependencies:
  - TASK-82
priority: high
project: Stack Ranking
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The switchbard-task surface for stack ranking (trajectory: 'Stack ranking'). Verb family: 'rank project <name>' and 'rank task <id>' with --top / --before <sibling> / --after <sibling>, plus 'unrank'; 'expedite <id>' / 'unexpedite <id>' for the exception lane. 'create' gains the same placement flags so a newly discovered task lands ranked among its siblings instead of reflexively expedited (owner insight: most queue-jumping was an incomplete queue). 'list', 'list --in-project', and 'project list' honor the computed order. Output contract stays agent-friendly: payload-only stdout, one-line stderr errors naming the next step.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 rank project/task with --top/--before/--after and unrank mutate ordering.yml through backlog/ordering.rs only
- [ ] #2 expedite/unexpedite manage the lane; expediting an unknown or terminal task id errors with a next step
- [ ] #3 create --rank-top/--rank-before/--rank-after places the new task among its siblings atomically with creation
- [ ] #4 list and project list output follows the computed order (ranked first, unranked by existing comparator), covered by CLI tests
<!-- AC:END -->
