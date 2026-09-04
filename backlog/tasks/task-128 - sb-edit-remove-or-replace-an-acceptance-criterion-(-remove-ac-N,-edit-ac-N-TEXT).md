---
id: TASK-128
title: 'sb edit: remove or replace an acceptance criterion (--remove-ac N, --edit-ac N TEXT)'
status: Done
assignee: []
created_date: '2026-09-01 22:43'
updated_date: '2026-09-04 00:12'
labels:
  - cli
  - tasks
dependencies: []
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Impact: `sb edit` can append (`--ac`), check, and uncheck acceptance criteria but cannot remove or reword one. An agent whose shell quoting mangles an AC (backticks in a double-quoted string ran as command substitution on 2026-09-01, leaving "Billing usage report shows zero  minutes" as budget LED-643 AC #2) has no repair path except appending a superseding criterion, so the board carries blank or wrong criteria forever and the roll-up counts them as unmet work. The golden rule forbids hand-editing the file, so today the only fix is no fix.
Evidence: `sb edit --help` lists --ac / --check-ac / --uncheck-ac only (crates/switchbard-task/src/main.rs:258-261, apply loop at :454); budget LED-643 acceptance criteria #2 vs #5, filed from session 2026-09-01.
Scope: add `--remove-ac <N>` (repeatable; remove by current number, then renumber the remaining criteria contiguously) and `--edit-ac <N> <TEXT>` (replace the text, preserve the checked state). Out-of-range N is a one-line stderr error naming the valid range. Apply order within one invocation: edits, then removals, then appends, so numbers in the same command refer to the pre-command numbering. Mirror in the switchbard SKILL.md at ~/.claude/skills/switchbard (outside this repo - note in the PR).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 sb edit <id> --remove-ac 2 deletes criterion #2 and the remaining criteria are renumbered #1..#n with no gap
- [x] #2 sb edit <id> --edit-ac 2 "new text" replaces the text of #2 and keeps its [x]/[ ] state
- [x] #3 Out-of-range or zero N fails with exit 1 and one stderr line naming the valid range; the file is untouched
- [x] #4 Tests in crates/switchbard-task/tests/cli.rs cover remove, edit, renumbering, and the error path
- [x] #5 cargo test, cargo clippy --all-targets -- -D warnings, and cargo fmt --check pass
<!-- AC:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
sb edit gained --edit-ac N TEXT (replace text, keep checked state) and --remove-ac N (repeatable, contiguous renumber), applied edits then removals then appends so numbers refer to pre-command numbering; out-of-range N exits 1 with one stderr line naming the valid range and leaves the file untouched. Covered in crates/switchbard-task/tests/cli.rs; the switchbard SKILL.md mirror lists both flags. Merged to main.
<!-- SECTION:FINAL_SUMMARY:END -->
