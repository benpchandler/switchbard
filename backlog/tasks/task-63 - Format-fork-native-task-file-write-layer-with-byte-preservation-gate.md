---
id: TASK-63
title: 'Format fork: native task-file write layer with byte-preservation gate'
status: Done
assignee:
  - '@claude'
created_date: '2026-08-28 18:40'
updated_date: '2026-08-28 18:57'
labels:
  - format-fork
dependencies:
  - TASK-62
priority: high
ordinal: 62000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
New backlog::write module in switchbard-core: surgical, atomic (write-tmp-then-rename, same pattern as config::save_to) mutations on task markdown files, no CLI subprocess. Surgical means an edit rewrites only the bytes of the field or section it targets; everything else survives byte-for-byte - unknown frontmatter keys (ordinal), key order, quoting style, author formatting. Any real change bumps updated_date; a no-op writes nothing at all. Body-structure edits fail closed on !body_round_trips, same contract refine already holds. NOT wired into mutations.rs yet - that is the swap task; this lands the layer plus its gate.

Milestone registry survey (done during scoping, 2026-08-28): backlog/milestones/ holds "m-N - slug.md" files; across all tracked repos only MusicProduction has any; tasks reference milestones by name in frontmatter. Registry writes are out of scope for this task.

Created files omit ordinal (CLI web-board ordering hint we cannot compute locally without a policy); decision recorded here, revisit in the swap task if the board is still used.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 backlog::write exposes scalar sets (status/priority/title/milestone), list-field sets (labels/assignee/dependencies/references), single-label add/remove, section replace (Description/Plan/Notes/Final Summary), notes append, AC append, AC/DoD check/uncheck, and new-task file creation, all without invoking the backlog CLI
- [x] #2 Byte-preservation gate runs over every real task file in this repo (tasks/completed/drafts/archive): raw split+rejoin is byte-identical, a no-op edit leaves the file byte-identical, and a status edit changes only the status and updated_date lines
- [x] #3 Frontmatter the model does not parse (ordinal, unknown keys) survives every edit byte-for-byte
- [x] #4 Body edits fail closed when body_round_trips is false
- [x] #5 Created files match the CLI on-disk shape (frontmatter key order, SECTION markers, '- [ ] #N' numbering) and are accepted by parse.rs
- [x] #6 mise run ci green
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Raw model: split task file into frontmatter lines / body lines with byte-exact rejoin (split(chr(10))+join is lossless; no-op detection compares full rebuilt text).
2. Frontmatter primitives: key-line span finder (scalar or block list), set-scalar, set-list, remove-key; single-quote YAML emitter matching CLI style (quote colons, dates, bool-likes; empty list inline []).
3. Body primitives: fence-aware section span finder (reuse parse.rs scan_fences via pub(super)), section replace with SECTION:X marker regeneration, canonical-order insert-if-missing, checklist line flip by #N index, AC append before AC:END with continued numbering.
4. Every mutation: validate inputs, transform, byte-compare, Unchanged (no write) or bump updated_date + atomic tmp+rename write (config::save_to pattern). Body edits guarded by task_file_round_trips, fail closed.
5. Create: full CLI-shaped template, create_new (collision-safe), filename convention from observed CLI output.
6. Tests: unit fixtures per op + integration gate over every real task file in this repo (no-op byte-identity, targeted-diff assertion, reparse equality via load_backlog_project on a tempdir project).
7. Wire: backlog/mod.rs pub mod write + explicit lib.rs re-exports. Not called by mutations.rs yet (TASK-65).
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Decisions made during implementation:
- Surgical text edits over serialize-from-a-model: the parsed model is lossy by design (ordinal, unknown keys), so byte preservation falls out of editing only the targeted lines instead of trying to make a full serializer faithful.
- The engine compares rebuilt bytes BEFORE bumping updated_date, which is what makes no-op-writes-nothing hold.
- parse.rs internals (scan_fences, heading_title, parse_checklist_index, KNOWN_SECTION_HEADINGS, yaml_string_list, parse_task_file) became pub(super) so the writer flips exactly the lines the reader counts - one authority, not a parallel parse.
- swap_task_label (the dispatch claim primitive) deliberately deferred to TASK-65: the CLI swap adds the target label even when the source label is absent, so claim semantics need defining at swap time - possibly stricter than CLI parity (fail the claim when the source label is gone).
- write_new_task_file omits ordinal (CLI web-board ordering hint; nothing in this app reads it, and a fresh task has no position to claim).
- Integration gate covers 44+ real task files: no-op byte-identity, status-edit line-diff confinement, and full-field reparse equality.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Landed backlog::write, the format fork's native write layer: surgical atomic mutations (scalar/list frontmatter fields, single-label toggles, marked-section replace/append, checklist check/uncheck with #N indexing, AC append with continued numbering, CLI-shaped task creation with create_new collision safety) with no CLI subprocess. No-op edits write nothing; real edits bump updated_date; body rewrites fail closed on non-round-tripping structure. Gated by unit fixtures plus tests/write_layer_real_files.rs over every real task file in this repo. Not yet wired into mutations.rs (TASK-65). mise run ci green.
<!-- SECTION:FINAL_SUMMARY:END -->
