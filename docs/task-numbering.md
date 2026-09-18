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

1. **One flat sequential number per repository, for every task including sub-issues.** Allocated once at creation, never reused, never changed. `TASK-312` is a sub-issue of `TASK-88` because its parent field says so. The sequence is per repository, not global; looking across repositories uses the repository's own abbreviation, which is what the configured task prefix already is (`TASK-`, `LED-`).
2. **Always write the parent field** on create and on move.
3. **Identity is the bare number; `88.312` is a display name.** The dotted form is computed from the parent field at render time and never persisted - not in a filename, not in `id:`, not in a dependency list, not in a branch name. A reparent changes what is displayed and nothing that is stored.
4. **The suffix is authoritative and the prefix is a hint.** `312`, `88.312` and a stale `60.312` from an old commit all resolve to the same task. A stale prefix resolves with a correction ("TASK-312, now under TASK-88") rather than failing - strictly better than today, where a stale reference resolves to nothing.
5. **Display shows one level of parent.** `60.88.312` is derivable and unreadable; nesting is capped at one level anyway.
6. **Reparent becomes a field update.** No rename, no dependency rewrite, no id remap, no stale selection. Most of `move_backlog_task` and the bulk-reparent remap in `crates/switchbard-tui/src/app/task_parent.rs` deletes.

## The one-time renumber

Existing decimal ids are folded into the flat sequence in creation order (owner-directed). This repository has nine of them - `TASK-80.1` through `80.4` including `80.2` in `completed/`, and `TASK-141.1` through `141.5`.

Evidence gathered 2026-09-17: none of the nine appears in any commit message on any branch, so no git history goes stale. They appear 101 times **inside** the repository - task bodies, `backlog/ranking.yml`, and other tasks' prose.

The migration therefore rewrites structural references (filename, `id:`, parent, dependencies, ranking) and **writes a permanent alias from each old id to its new one**. The resolver honors aliases forever, so the prose mentions keep resolving and no historical note has to be edited to stay correct. Aliases are append-only: an id that ever named a task keeps naming it.

This renumber is the only one. Live renumbering on any ongoing rule is rejected below.

## Rejected: live renumbering

Any rule that renumbers on a change - by priority, by project, by position - reintroduces exactly the instability the number exists to avoid. The number's whole job is to survive in the places nothing can rewrite.

The real need behind "renumber so I can refer to it quickly" is a **per-view row number that is never stored**: press `3` to act on the third visible row, meaningless tomorrow. `sbt` already has digit keys in that shape. Ephemeral reference, permanent identity.

## Rejected for now: a separate UUID

The number is already unique per repository and stable for life, so it is a perfectly good key. Cross-repository exchange is already solved by a repository-identity UUID plus the task id as a composite (`storage rebind --repo-id UUID`, `docs/central-storage.md`).

A second, user-visible identifier would mean two answers to "which task is this", which is the failure shape this repository keeps hitting. An internal surrogate earns its place only if two repositories' backlogs merge into one namespace, and that is the moment to add it, with a display-number migration named explicitly rather than assumed.

## Settled, and what is not implied

Settled: the sequence is per repository, and cross-repository views prefix the repository's abbreviation. Sub-issues draw from the same sequence as top-level tasks, so promoting one is also just a field change.

To verify while implementing: every place that parses a decimal id to infer a parent must route through the one resolver, or it becomes a competing authority. `crates/switchbard-core/src/backlog/ranking.rs` is the known one.

Not implied: nesting is capped at one level today because core refuses a sub-issue as a parent. Flat ids remove the structural reason for that cap, but whether to lift it is a separate product question.
