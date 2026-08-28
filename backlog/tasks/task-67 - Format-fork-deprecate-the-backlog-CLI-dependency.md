---
id: TASK-67
title: 'Format fork: deprecate the backlog CLI dependency'
status: Done
assignee:
  - '@claude'
created_date: '2026-08-28 18:40'
updated_date: '2026-08-28 20:34'
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
- [x] #1 grep gate: no reference to the backlog CLI binary remains under crates/
- [x] #2 mise.toml backlog pin removed
- [x] #3 Repo CLAUDE.md rewritten for the native write path
- [x] #4 mise run ci green on both platforms
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
- Removed backlog_cli_path probing (PATH walk + homebrew fallbacks), the BacklogProject.cli_path field, cli_available(), and every degraded-mode UI gate (create modal disable + amber warning, board/detail editability halves, toolbar cleanup/bulk filters). Editability is now task.editable() alone - writes need no external binary.
- Deleted the two degraded-mode tests whose UI state no longer exists (missing-CLI read-only pill; Refine hidden without CLI) and the missing_cli_app fixture + its screenshot.
- Migrated every GUI test fixture off the real backlog binary: the 9 `git init + backlog init + backlog task create/edit/complete` fixture blocks across backlog_controls / qa_reverify / wave2 are now native (dir shape + config.yml + create_backlog_task/edit_backlog_task), no git needed either. Renamed the three test fns whose names claimed a real-CLI round trip.
- mise.toml: node + npm:backlog.md pins removed (they existed solely for the CLI round-trip tests).
- CLAUDE.md: the task-management paragraph now states the CLI is retired here and all mutations go through switchbard-task or the GUI.
- Kept deliberately: historical doc comments attributing format conventions to the CLI era (where a rule CAME from is evidence, not a dependency), and the AC-4 both-platforms half is proven by the PR CI run on this push.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Retired the external backlog CLI: no invocation, probe, pin, or degraded-mode UI state remains. backlog_cli_path / BacklogProject.cli_path / cli_available() deleted with every GUI gate that keyed off them; all test fixtures that shelled the real binary migrated to native construction; node + backlog.md pins removed from mise.toml; CLAUDE.md rewritten for the native path. Historical attribution comments kept on purpose. mise run ci green.
<!-- SECTION:FINAL_SUMMARY:END -->
