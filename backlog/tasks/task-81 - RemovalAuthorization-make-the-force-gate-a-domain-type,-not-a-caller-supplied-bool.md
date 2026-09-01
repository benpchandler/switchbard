---
id: TASK-81
title: 'RemovalAuthorization: make the force gate a domain type, not a caller-supplied bool'
status: To Do
assignee: []
created_date: '2026-08-31 21:49'
labels:
  - core
  - hardening
dependencies: []
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
remove_worktree(repo, path, force: bool) and delete_branch(repo, branch, force: bool) in switchbard-core/src/worktree_remove.rs accept force as a bare bool. The invariant 'only RemovalVerdict::Safe may be acted on without an explicit force gesture' is therefore upheld by convention at the GUI call sites (worktree_actions.rs, ui/workspace/mod.rs, runtime/mod.rs), not owned by core. This is the same one-fact-two-sources shape that removal_safety.rs was created to kill for the verdict itself: the verdict has one definition, but the authorization to act on it is re-derived per caller. Any future frontend (TUI, dispatch extension) would have to re-implement the gating correctly by hand.

Refactor: introduce a domain type (e.g. RemovalAuthorization) in removal_safety.rs, constructible only via (a) a Safe verdict or (b) an explicit user-force token that callers must mint from a deliberate gesture. remove_worktree and delete_branch take the authorization instead of bool; the bool disappears from the public surface. Wrong callers stop compiling. Rule 2: assert the invariant at its owning boundary.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 remove_worktree and delete_branch no longer accept a raw force bool in their public signatures; they require an authorization type owned by removal_safety
- [ ] #2 The authorization type cannot be constructed from an unsafe verdict without an explicit force value that call sites must produce deliberately (no Default, no bool-to-auth From impl)
- [ ] #3 All GUI call sites compile against the new signature with unchanged user-visible behavior (row badge, bulk sweep, and confirm dialog act exactly as before)
- [ ] #4 Core unit tests cover: Safe verdict authorizes without force; non-Safe verdict refuses without force; explicit force authorizes; and the dirty-worktree remove tests still pass under the new API
- [ ] #5 mise run ci green on the branch
<!-- AC:END -->
