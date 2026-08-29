---
id: TASK-66
title: 'Format fork: thin switchbard task CLI frontend'
status: Done
assignee:
  - '@claude'
created_date: '2026-08-28 18:40'
updated_date: '2026-08-28 20:26'
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
- [x] #1 Command surface covers the lifecycle agents use today per the backlog-cli skill: view, list, edit fields, check AC/DoD, append notes, final summary, status moves, create
- [x] #2 agent-facing-design.md reviewed and its named failure modes addressed in the command design
- [x] #3 Repo CLAUDE.md points agents at the new CLI as the write path
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
- New crates/switchbard-task bin (clap derive; workspace gains clap 4). Commands: list (TSV: id/status/priority/labels/title), view (fields then sections), create (stdout = the id alone), edit (patch fields + --add/remove-label + --check/uncheck-ac/dod + --append-notes + --final-summary), archive, complete. Project resolution: nearest ancestor with backlog/, or --project.
- agent-facing-design applied: stdout is payload-only; every error is one stderr line carrying the next step (missing task -> suggests list; no project -> names --project) with exit 1; the long help documents ids, project resolution, and the exact stdout contract per subcommand; nothing blocks, so banner/heartbeat rules do not apply.
- Added mutations::set_backlog_final_summary (lifecycle wrap-up field; BacklogTaskPatch deliberately does not carry it - written once at the end, not composed with other edits).
- 11 tests: 5 unit (id normalization, bounded root walk, clap self-check, render shapes) + 6 end-to-end driving the real binary via CARGO_BIN_EXE (full lifecycle round trip, TSV stability, dispatch flag toggle, error shapes, escape-hatch naming, decimal subtask id).
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Landed switchbard-task, the terminal/agent frontend over the native write layer - the format fork's replacement for the backlog CLI's write path and the keeper of dispatch's flaggable-from-a-plain-terminal property. Full lifecycle surface (list/view/create/edit/check-ac/check-dod/append-notes/final-summary/status/archive/complete/label toggles), designed against agent-facing-design.md (payload-only stdout, error+next-step stderr lines, help-text-as-contract). Repo CLAUDE.md now points agents at it. mise run ci green.
<!-- SECTION:FINAL_SUMMARY:END -->
