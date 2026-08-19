# Switchbard — "Stale worktree sweep" feature

## Objective
Make Switchbard the place Ben sees and retires dead worktrees, so that
"which of my 138 worktrees are merged/orphaned, how big are they, and kill the
safe ones in bulk" is a view + one action — not an ad-hoc script in a chat.

## Context (verified 2026-08-19)
- `~/Dev/.worktrees` holds **138 worktrees / 141 GB**. Classified against
  `origin/main` with `git merge-base --is-ancestor HEAD origin/main`:
  **89 merged · 17 no-upstream (orphan) · 30 live · 36 dirty** (any class).
  By repo: budget 52, sospw 37, musicproduction 22, matterline 12,
  inspiration 6, ehg 4, switchbard 3.
- Workspace view today shows per-worktree: branch, dirty/clean, ahead/behind
  **upstream** (`DriftProbe`, `crates/switchbard-core/src/git_probe.rs`),
  activity, listeners. It does **not** show merged-into-main, size, or any
  bulk action.
- The merged check already exists but only inside the Remove dialog:
  `assess_branch_delete` / `BranchDeleteAssessment` in
  `crates/switchbard-core/src/worktree_remove.rs` (blocked if checked out
  elsewhere; force-required if unmerged; `compared_against` names the base).
  Dialog UI: `crates/switchbard-gui/src/ui/workspace/mod.rs` ~L1485–1500.
- Board already has bulk select/edit (TASK-26) — reuse its selection pattern.
- `~/.switchbard/config.toml` was restored today with 11 repos (TASK-22 incident
  had wiped it). If the file is empty again after an app quit, that is the
  TASK-22 repro — note it, don't absorb it.

## Deliverable
1. **Core:** a `WorktreeStaleness` probe — `Merged { base } | Orphan (no upstream)
   | Live` — computed per worktree alongside `DriftProbe`, plus on-disk size
   (du, cached, refreshed lazily). Pure function of (repo_path, worktree_path).
2. **UI:** badge + size column in Workspace worktree rows; filter chips
   (All / Merged / Orphan / Live / Dirty).
3. **Bulk remove:** multi-select → one Remove dialog that reuses per-row
   `collect_dirty_files` + `assess_branch_delete`. Clean+merged rows are
   removable (with "also delete branch" default on, since merged); dirty or
   unmerged rows are **auto-deselected into a "needs review" list**, never
   force-removed in bulk.
4. **Nudge:** a top-bar count "N retired worktrees" when merged+clean > 0.

## Constraints
- Follow `~/Dev/switchbard/CLAUDE.md`, AGENTS.md, power-of-10-overrides.md.
- Never force-remove or delete an unmerged branch in bulk. Primary worktree
  (`is_primary_worktree`) is never selectable.
- Rust; tests for the probe (merged / orphan / live / dirty fixtures) and a
  kittest for bulk dialog auto-deselect behavior.
- Work in a worktree; PR against main; run `mise run ci`.
- File via Backlog first: `backlog task create` in `~/Dev/switchbard` with
  AC + DoD from this doc; reference the task id in the PR.

## Acceptance
- Open the app → Workspace shows Merged/Orphan/Live badges and sizes for all
  tracked repos' worktrees; filter "Merged" lists ≈89 today.
- Select 5 merged+clean worktrees → Remove → all 5 gone, branches deleted,
  `git worktree list` agrees; a dirty one included in the selection is shown
  under "needs review" and untouched.
- CI green; CHANGELOG entry.

## Out of scope
- The one-time cleanup of today's 89/17 (Ben may do that via git separately).
- Dispatch/launchd work (TASK-11/12).
