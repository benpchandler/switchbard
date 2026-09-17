# Task numbering: stable ids, hierarchy as a field

Owner-directed 2026-09-17. Status: proposed direction, not implemented. Tracking: to be filed.

## What a task number is for

One job: letting a person name a task to an agent, in a sentence, from memory. "Work on TASK-145." That sentence is typed into terminals, commit messages, branch names, PR titles and chat, none of which anything can rewrite later. A number that changes breaks every one of those references silently, because they are text, not links.

Everything else a number might do - sort into outline order, show lineage, group by project - is display, and display can be computed from the current state on every render. Identity cannot.

## Current scheme and its one problem

A task id is `TASK-<n>`, and a sub-issue is a decimal child of its parent: `TASK-12.1`. The id therefore encodes position in the hierarchy, which reads well in a plain-text row and sorts into an outline for free.

The cost is that moving a task has to rename it. `move_backlog_task` (`crates/switchbard-core/src/backlog/mutations.rs`) claims a new id under the new parent, rehomes the file, and rewrites every other task's dependency list that referenced the old id. References it cannot reach - commits, branches, notes, human memory - keep naming a task that no longer answers to that name.

Two facts make this cheaper to fix than it looks:

- `parent_task_id` frontmatter already exists and already wins. Parsing `TASK-7.2` to `TASK-7` is only the fallback when the field is absent (`crates/switchbard-core/src/backlog/ranking.rs`). Hierarchy does not depend on the number; the number is just where it currently gets recorded by default.
- Central storage already assumes what this proposes: "resolved task references follow stable identities across reparenting" (`docs/central-storage.md`). Today's rename-on-move contradicts that sentence.

## Direction

1. **One flat sequential number per repository, for every task including sub-issues.** Allocated once at creation, never reused, never changed - a one-way door. `TASK-312` is a sub-issue of `TASK-88` because its parent field says so.
2. **Always write the parent field** on create and on move. Keep decimal-id parsing as a read-only fallback so existing ids keep resolving.
3. **Reparent becomes a field update.** No rename, no dependency rewrite, no id remap, no stale selection. Most of `move_backlog_task` and the bulk-reparent remap in `crates/switchbard-tui/src/app/task_parent.rs` deletes.
4. **Existing decimal ids keep their names forever.** They are already unique and already stable; nothing migrates. The repository simply stops minting new ones.
5. **Show lineage instead of encoding it**: indentation in the outline and a parent chip on the row and in `sb view`, computed from the parent field the way roll-up and goal pace already are.

## Rejected: live renumbering

Any rule that renumbers on a change - by priority, by project, by position - reintroduces exactly the instability the number exists to avoid. The number's whole job is to survive in the places nothing can rewrite.

The real need behind "renumber so I can refer to it quickly" is a **per-view row number that is never stored**: press `3` to act on the third visible row, meaningless tomorrow. `sbt` already has digit keys in that shape. Ephemeral reference, permanent identity.

## Rejected for now: a separate UUID

The number is already unique per repository and stable for life, so it is a perfectly good key. Cross-repository exchange is already solved by a repository-identity UUID plus the task id as a composite (`storage rebind --repo-id UUID`, `docs/central-storage.md`).

A second, user-visible identifier would mean two answers to "which task is this", which is the failure shape this repository keeps hitting. An internal surrogate earns its place only if two repositories' backlogs merge into one namespace, and that is the moment to add it, with a display-number migration named explicitly rather than assumed.

## Open questions

- Does the flat sequence allocate per repository or per central database? Per repository keeps `TASK-<n>` short and matches today's `configured_task_prefix`; the central database would have to tolerate the same number in two repositories, which the repo-identity composite already does.
- Does anything outside `switchbard-core` parse a decimal id to infer a parent? The fallback must stay for existing ids either way, but a second parser would be a competing authority.
- Nesting is capped at one level today (core refuses a sub-issue as a parent). Flat ids remove the structural reason for that cap. Whether to lift it is a separate product question, not implied by this change.
