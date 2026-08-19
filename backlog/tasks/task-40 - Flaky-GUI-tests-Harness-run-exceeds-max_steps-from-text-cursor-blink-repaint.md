---
id: TASK-40
title: 'Flaky GUI tests: Harness::run exceeds max_steps from text-cursor blink repaint'
status: Done
assignee: []
created_date: '2026-08-19 19:28'
updated_date: '2026-08-19 19:52'
labels:
  - test-flake
dependencies: []
ordinal: 40000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
crates/switchbard-gui/tests/backlog_controls.rs: append_note_button_clears_the_note_input and references_add_button_clears_the_input_field intermittently panic locally with 'Harness::run exceeded max_steps (4)' (repaint cause: egui text_selection/visuals.rs cursor blink) after focusing a text field and typing. Timing-dependent; seen ~1 in 3 local runs on 2026-08-19, not yet on CI. Likely fix: use run_steps / disable cursor blink in the harness style, or avoid leaving the field focused before run().
<!-- SECTION:DESCRIPTION:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Fixed in 3e0f08a (merged to main via #22): harness disables text_cursor.blink; theme::apply preserves the host's blink flag. 10/10 local stress runs + CI green on macOS and Ubuntu.
<!-- SECTION:NOTES:END -->
