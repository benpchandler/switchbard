---
id: TASK-67
title: 'Format fork: deprecate the backlog CLI dependency'
status: To Do
assignee: []
created_date: '2026-08-28 18:40'
labels:
  - format-fork
dependencies:
  - TASK-65
  - TASK-66
priority: medium
ordinal: 66000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Remove every remaining trace of the external CLI: backlog_cli_path probing and homebrew fallbacks, cli_available degraded modes and their UI states, the mise.toml backlog pin. Retire backlog MCP/skill usage for tracked repos: rewrite repo CLAUDE.md guidance; owner updates user-level skill guidance outside this repo. Read-only external tools (the backlog web board) may keep working against the files; nothing in switchbard invokes the CLI after this task.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 grep gate: no reference to the backlog CLI binary remains under crates/
- [ ] #2 mise.toml backlog pin removed
- [ ] #3 Repo CLAUDE.md rewritten for the native write path
- [ ] #4 mise run ci green on both platforms
<!-- AC:END -->
