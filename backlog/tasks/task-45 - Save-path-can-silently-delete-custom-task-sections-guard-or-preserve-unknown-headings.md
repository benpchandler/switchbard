---
id: TASK-45
title: >-
  Save path can silently delete custom task sections (guard or preserve unknown
  headings)
status: Done
assignee: []
created_date: '2026-08-20 03:32'
updated_date: '2026-08-31 11:21'
labels: []
dependencies: []
priority: high
ordinal: 45000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Measured during TASK-44: 51 of 345 real task files (all in the budget repo, zero in switchbard) carry human-written sections the Backlog format has no field for (## Resolution, ## Root Cause Hypothesis, ## Reproduction Steps). parse_task_file extracts six known sections; content under any other heading lands in no BacklogTask field, so the detail rail's Save (-d replace-write from the parsed description) genuinely deletes those sections today. Refine is guarded by task_file_round_trips since PR #25; Save is not. Two candidate fixes, product call required: (a) wire Save through the same round-trip guard — safe but starts refusing saves on ~15% of real tasks; (b) the real fix: make the parse/write cycle round-trip-complete by preserving unknown sections as opaque blocks and re-emitting them on write, then Save never needs to refuse. Prefer (b) unless its CLI write path can't express it.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 A Save on a task file carrying an unknown ## section cannot delete that section
- [x] #2 Decision between refuse-vs-preserve recorded in docs/product-trajectory.md with rationale
- [x] #3 Regression test with a custom-heading fixture on the Save path
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Relax parse::body_round_trips rule 2: allow unique unknown headings (opaque blocks the surgical writer never touches), refuse any repeated heading; fences/rule3/conservation unchanged.\n2. Update stale docs: refine.rs residual ('Save does not consult the guard' is false since TASK-65) + guard description; write.rs refusal message; trajectory doc decision record (preserve, not refuse).\n3. Regression tests: parse unit tests (unique unknown accepted, repeats refused), write.rs surgical-preservation test, Save-path test in backlog_mutations.rs with a custom-heading fixture.\n4. mise run ci green.
<!-- SECTION:PLAN:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Resolved refuse-vs-preserve as PRESERVE (option b), enabled by the surgical write layer: body_round_trips rule 2 no longer refuses unknown headings (only repeated headings, unbalanced fences, and un-owned preamble still fail closed), and since every section edit rewrites only its own span, an unknown heading's block is opaque and survives byte-for-byte. AC#1/#3: proven end-to-end by saving_a_description_preserves_custom_sections_the_format_does_not_model (backlog_mutations.rs, the exact edit_backlog_task route the detail rail's Save takes) and replacing_a_section_preserves_an_unknown_section_byte_for_byte (write.rs), plus parse-level tests for the accept/reject split. AC#2: decision + rationale recorded in docs/product-trajectory.md under the Refine guard bullets, which also closes the stale 'Save does not consult the guard' residual (false since TASK-65). Stale docs in refine.rs corrected. mise run ci green.
<!-- SECTION:FINAL_SUMMARY:END -->
