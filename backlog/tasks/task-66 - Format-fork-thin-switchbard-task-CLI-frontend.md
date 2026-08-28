---
id: TASK-66
title: 'Format fork: thin switchbard task CLI frontend'
status: To Do
assignee: []
created_date: '2026-08-28 18:40'
labels:
  - format-fork
dependencies:
  - TASK-65
priority: medium
ordinal: 65000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Preserve the "flaggable from a plain terminal with no Switchbard" dispatch property and give agents a write path once the backlog CLI is retired. Thin binary (same pattern as switchbard-dispatch) over the switchbard-core write layer: view, list, create, edit, check-ac/check-dod, append-notes, label add/remove, plain output for agents. This is an agent-facing interface: read ~/.claude/standards/agent-facing-design.md before building, per code-standards.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Command surface covers the lifecycle agents use today per the backlog-cli skill: view, list, edit fields, check AC/DoD, append notes, final summary, status moves, create
- [ ] #2 agent-facing-design.md reviewed and its named failure modes addressed in the command design
- [ ] #3 Repo CLAUDE.md points agents at the new CLI as the write path
<!-- AC:END -->
