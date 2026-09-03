---
id: TASK-135
title: 'sb edit --parent <ID>|none: reparent or promote a task, re-minting its decimal id and fixing references'
status: Done
assignee: []
created_date: '2026-09-02 21:51'
updated_date: '2026-09-02 22:03'
labels:
  - cli
  - tasks
  - hierarchy
dependencies:
  - TASK-133
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Impact: a task's parent is baked into its id (TASK-9.4), filename, and parent_task_id: frontmatter, so moving a sub-issue under another parent or promoting it to top level today means create-new + archive-old, losing id continuity and every dependency/rank/goal reference. Needed 2026-09-02 to move shared-document sub-issues out from under a lender-specific parent in the CambridgeKitchens backlog. Evidence: allocate.rs first_candidate (one decimal level), write.rs write_new_task_file filename shape, ranking.yml subissues/root_tasks/tasks lists and goals.yml inputs.tasks referencing ids.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 switchbard_core::move_backlog_task(root, id, new_parent: Option<&str>) allocates a new id via the reservation allocator (decimal under the new parent, or top-level), writes the file under its new name with id:/parent_task_id: updated, removes the old file, and returns the new id
- [x] #2 refuses: task has sub-issues; new parent is itself a sub-issue; new parent is the task itself; new parent does not exist; no-op when the parent is unchanged
- [x] #3 references follow: other tasks' dependencies, ranking.yml (expedite renamed; the moved id dropped from its old sibling list), goals.yml inputs.tasks
- [x] #4 sb edit <ID> --parent <ID>|none prints Moved <OLD> -> <NEW>
- [x] #5 unit tests cover reparent, promote, each refusal, and reference rewriting
<!-- AC:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
move_backlog_task() in mutations.rs: claim_task_id() (allocate.rs, shared with create) mints the new id; rehome_task_file() (write.rs) rewrites id:/parent_task_id: and renames the file create_new-then-remove; dependencies in other tasks, ranking.yml (rename_task_in_ranking: expedite renamed, old sibling scope dropped) and goals.yml (rename_task_in_goals) follow. sb edit --parent <ID>|none prints Moved OLD -> NEW after other edits. 3 tests. Gates clean, 511 core tests.
<!-- SECTION:FINAL_SUMMARY:END -->
