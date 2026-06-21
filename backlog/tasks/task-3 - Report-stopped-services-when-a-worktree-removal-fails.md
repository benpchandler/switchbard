---
id: TASK-3
title: Report stopped services when a worktree removal fails
status: Done
assignee:
  - '@removal-fixes'
created_date: '2026-06-21 01:49'
updated_date: '2026-06-21 01:56'
labels:
  - mutation-arch
  - audit
  - observability
dependencies: []
priority: high
ordinal: 3000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Audit finding #3. execute_remove_worktree (crates/switchbard-gui/src/app.rs:476-544) kills tracked services and drops them from active_runs (app.rs:477-495) BEFORE 'git worktree remove' (app.rs:499). The '(stopped N services)' note is built only in the Ok arm (app.rs:516-523); the Err arm (app.rs:538-543) sets state.error to git's message alone. A failed removal therefore silently leaves the user's dev servers dead with no mention the kill happened. No compensation is possible (can't un-kill), so honest reporting is the fix.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 On the Err path, the dialog error includes the count of services stopped (e.g. 'stopped N service(s), but removal failed: <git err>')
- [x] #2 Wording matches the Ok-path note's singular/plural handling
- [x] #3 A unit/extracted-fn test covers the failed-removal-after-kill message
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Extracted removal_error_message(killed, git_error) -> String into worktree_actions.rs (pub fn). In Err arm of execute_remove_worktree, replaces the bare e.to_string() call. Wording: 0 killed -> verbatim git error; 1 killed -> 'stopped 1 service, but removal failed: ...'; N killed -> 'stopped N services, but removal failed: ...'. Matches singular/plural of Ok-arm. Test file: tests/worktree_removal_error.rs with three cases.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Extracted removal_error_message(killed, git_error) into a testable pub fn in worktree_actions.rs and used it in the Err arm of execute_remove_worktree. When killed > 0, the dialog now shows 'stopped N service(s), but removal failed: <git err>' with correct singular/plural. New test file tests/worktree_removal_error.rs covers zero, one, and many killed. CI green.
<!-- SECTION:FINAL_SUMMARY:END -->
