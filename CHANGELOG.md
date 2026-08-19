# Changelog

All notable changes to Switchbard are documented here. Switchbard is alpha
software; versions follow [Semantic Versioning](https://semver.org/) loosely
within the `0.x` line (minor = new features, patch = fixes).

## [Unreleased]

### Added

- **Stale worktree sweep (TASK-41).** The Workspace view now shows a
  Merged/Orphan/Live staleness badge and an on-disk size label on every
  worktree row, plus All/Merged/Orphan/Live/Dirty filter chips (with live
  counts) above the repo cards. A checkbox on each non-primary row feeds a
  "Remove N selected…" bulk action: the confirmation dialog re-checks every
  selection with the same primitives the single-row Remove dialog uses
  (`collect_dirty_files` + `assess_branch_delete`), routes anything dirty,
  unmerged, or still running services into a "needs review" list that's
  never touched, and only ever removes clean + fully-merged worktrees —
  never force, in bulk or otherwise. A top-bar "N retired worktrees" nudge
  appears whenever at least one clean, merged worktree is sitting around.
  Size is `du`-based, cached, and refreshed lazily on its own background
  cadence (see `workers.rs`'s cadence-policy doc) — it's an order of
  magnitude more expensive than any other per-worktree probe, so it never
  runs on the main git-probe tick.

## [0.3.0] - 2026-06-05

Worktree lifecycle release: create, rename, and remove worktrees — including
optional branch cleanup — without leaving the app.

### Added

- **Create worktree in-app.** A `+ Worktree` action on each repo opens a dialog
  to check out a new worktree from a new or existing branch. Switchbard suggests
  a name, location, and base, and validates against duplicate names and existing
  paths before shelling out to `git worktree add`.
- **Rename worktree labels.** Each worktree row has a `Rename` action for its
  Switchbard-local display name (an alias persisted in `~/.switchbard/config.toml`),
  so long or cryptic branch names don't have to be how you identify a worktree.
- **Delete the branch when removing a worktree.** The remove dialog now offers an
  opt-in "also delete branch" checkbox with the same "safe to remove" reasoning
  as the worktree itself:
  - blocked outright when the branch is checked out in another worktree
    (including the primary checkout);
  - a plain delete when the branch is fully merged into the default branch;
  - a loud, explicit force-delete — spelling out how many commits would be lost —
    when the branch has unlanded work.

  Worktree removal always happens first; branch deletion is reported separately
  so a removed worktree is never left in a half state. The remote branch is never
  touched.

### Changed

- **Worktree row layout.** The branch name moved from the collapsed header into
  the expanded row, so long branch names no longer crowd or overlap the
  Rename / remove actions. It truncates with a hover-to-reveal tooltip.

### Internal

- Optional frame/render performance telemetry behind `SWITCHBARD_PERF`, with a
  `scripts/perf-summary.py` summarizer and durable run records under `docs/perf/`.
- A local `Stop` hook (`scripts/rebuild-and-reload.sh`) that rebuilds the DMG and
  reloads the app when Rust sources change.

## [0.2.0] - 2026-05-22

### Added

- **Remove worktree action.** A trash icon on every non-primary worktree row
  opens a confirmation dialog that enumerates uncommitted changes and
  Switchbard-tracked services, with the action button labeled for what's at stake.
- Preflight re-snapshot at confirm time so files written between dialog-open and
  confirm aren't silently `--force`-removed. The primary worktree is refused.

## [0.1.1] - 2026-05-21

- Alpha packaging and install fixes for the macOS DMG.

## [0.1.0]

- First alpha: listener attribution, service detection, per-worktree git state,
  and the start / stop / kill / open control surface.

[0.3.0]: https://github.com/benpchandler/switchbard/releases/tag/v0.3.0
[0.2.0]: https://github.com/benpchandler/switchbard/releases/tag/v0.2.0
[0.1.1]: https://github.com/benpchandler/switchbard/releases/tag/v0.1.1
[0.1.0]: https://github.com/benpchandler/switchbard/releases/tag/v0.1.0
