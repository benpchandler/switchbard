---
id: TASK-95
title: 'sb add <title>: quick capture that falls back to the hub repo outside a Backlog repo'
status: To Do
assignee: []
created_date: '2026-09-01 02:20'
labels:
  - cli
  - dx
dependencies: []
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Impact: quick task capture only works inside a Backlog repo; run anywhere else, sb create hard-errors, so ideas captured from arbitrary directories require remembering --repo ~/Dev/hub or get lost. Owner requested this verbatim on 2026-09-01: a command that adds a task to whatever repo it is run from, or to hub when the cwd is not a recognized repo.

Evidence: sb create 'test' from a non-repo directory exits 1 with 'no Backlog repo found at or above ...'. The hub concept already exists: switchbard-core backlog_triage.rs find_hub_repo locates the tracked repo containing ordering.yml, and ~/Dev/hub (tracked in ~/.switchbard/config.toml) has both backlog/ and ordering.yml, so the fallback target is resolvable from existing config with no new configuration surface.

Decision needed: (a) new 'add' verb that is create-with-fallback (create stays strict, scripts keep failing loudly), (b) make create itself fall back with a printed notice of which repo received the task, or (c) 'add' as a pure alias of create plus fallback in both. Leaning (a) keeps the agent-facing contract strict while giving humans the forgiving verb; whoever implements should record the choice.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 From inside any Backlog repo, sb add 'x' creates the task in that repo (same id/output contract as create)
- [ ] #2 From a directory with no Backlog repo ancestor, sb add 'x' creates the task in the hub repo (located via find_hub_repo over tracked repos) and stderr names the repo that received it
- [ ] #3 With no hub repo resolvable, sb add fails with a one-line error naming the fix (track a repo with ordering.yml or pass --repo)
- [ ] #4 CLI tests cover all three paths
<!-- AC:END -->
