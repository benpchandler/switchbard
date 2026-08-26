# Changelog

All notable changes to Switchbard are documented here. Switchbard is alpha
software; versions follow [Semantic Versioning](https://semver.org/) loosely
within the `0.x` line (minor = new features, patch = fixes).

## [0.4.0] - 2026-08-26

Backlog and Dispatch release. Switchbard grew from a worktree/process dashboard
into a workbench: it now reads the `backlog` CLI's tasks out of every connected
repo, and can hand one to a headless agent and track the run to its PR.

### Added

- **Backlog view.** Tasks from every connected repo, in one place, read through
  the `backlog` CLI rather than by parsing files behind its back. Six lenses over
  the same data - List, Board, Milestones, Statistics, Digest, and Portfolio -
  plus global search and cross-repo triage ranking driven by a hub repo's
  `ordering.yml`.
- **Board.** Drag a card between columns for an optimistic status change that
  renders on the drop frame and rolls back if the CLI write fails. Icebox column
  (the union of tracked repos' non-standard statuses), bulk select and bulk edit,
  labels and age on cards, and click-through to the task detail.
- **Task authoring.** Create a task with labels, assignee, milestone, and
  dependencies set up front, rather than creating it and then editing it four
  times.
- **Sub-tasks, dependencies, and saved views.** A collapsible sub-task tree with
  roll-up progress, blocked/blocking visibility feeding triage, and named
  filter + sort + lens combinations you can return to.
- **Dispatch.** Label a task and Switchbard takes it from there: claim it, cut a
  worktree, run `claude -p` against it, and open a PR. `switchbard-dispatch` is a
  headless binary over the same pipeline with a launchd template, so the queue
  drains with the GUI closed.
- **Dispatch view.** Every run's state, with an ambient chip and a tab badge so an
  in-flight run is visible from anywhere in the app, and a Kill control that only
  appears when the run's process group can be positively identified.
- **Refine.** An AI-assisted grooming step upstream of Dispatch that fills a
  task's description, acceptance criteria, and plan through a read-only headless
  run, applied additively through the `backlog` CLI.
- **Stale worktree sweep.** A Merged/Orphan/Live staleness badge and an on-disk
  size label on every Workspace row, All/Merged/Orphan/Live/Dirty filter chips
  with live counts, and a bulk "Remove N selected…" action. Size is `du`-based and
  refreshed on its own slow cadence, since it is an order of magnitude more
  expensive than any other per-worktree probe.
- **Configurable staleness filter and bulk clear in Backlog.** Sweep by age with a
  threshold you set, then clear what is showing - routed by disposition, so Done
  tasks are *completed* and the rest are *archived*, since those are different
  terminal states in the `backlog` CLI and not interchangeable.
- **Determinate progress for bulk actions.** A batch that makes one CLI call per
  task now shows how far through it is, instead of several seconds of apparent
  nothing that reads as a hang.
- **Persistent contextual detail rail**, replacing click-to-List navigation.
- **Linux release artifacts** alongside the macOS DMG.

### Changed

- **One definition of "safe to remove a worktree."** It previously existed in
  three places that disagreed: the Workspace row badge ran three checks, the bulk
  sweep ran five, and the remove dialog reasoned a third way. The same worktree
  could read "remove ok" in green on the row and land in the sweep's "needs
  review" list in the same frame. All three now evaluate one rule set, and only a
  fully clear verdict is ever acted on without an explicit force gesture - an
  unanswered check never counts as a passed one.
- **"Has this work landed" is now content-based.** The check asked whether a
  branch's commits were reachable from the trunk, which reports "3 commits
  unlanded" about work a rebase merge already put there under different commits.
  It also compared against the local trunk, which on a machine that dispatches
  agents is routinely behind. Both are fixed: patch equivalence, and the
  remote-tracking ref where one exists. Measured on an 11-repo machine, removable
  worktrees went from 8 to 19.
- **egui 0.30 -> 0.31 -> 0.36.** The 0.36 step is an architectural port: panels
  became a unified `Panel` API and `App::update` became `App::ui`.
- **Dark theme has real surface hierarchy.** Five surfaces had been packed into a
  luminance range so narrow the rail and the board measured 1.02:1 against each
  other, which is what "flat" meant.
- **Unified status vocabulary and ordering** across every view, so a status means
  the same thing and sorts the same way wherever it appears.
- **Backlog toolbar** is one container with one count, its filters act as a single
  faceted group (each control's options exclude only its own facet), and Enter
  saves.
- **Dispatch no longer kills a run on a wall clock.** Staleness is advisory; a
  slow run is reported, not executed.
- **Scan cadence tuned** after a measured audit: the git-probe tick went from
  ~37-40s to ~6-8s.

### Fixed

- **A dispatched agent's worktree no longer looks idle.** The "nothing running
  here" check counted only attributed listeners and services Switchbard itself
  started, and a headless agent is neither.
- **A locked worktree no longer reads as removable.** `git worktree list` reports
  the flag; Switchbard parsed it and threw it away, then git refused a removal the
  badge had already blessed.
- **A failed staleness probe no longer nominates a worktree for cleanup.** It fell
  through to "orphan" - the most retire-me-looking badge there is - on no evidence
  at all.
- **A finished dispatch run's clock stops.** A 30-minute run that ended five days
  ago rendered as "ran 132h 33m" under a section that by definition only holds
  finished runs.
- **A dispatch PR link is a link again**, rather than the whole trailing line
  rendered as one.
- **Board cards no longer stretch their column.** An untruncated label list
  widened the card, and the column with it.
- **Done tasks route through Complete, not Archive.** They are different terminal
  dispositions in the `backlog` CLI.
- **One backlog project per repo**, not per worktree.
- **Board card clicks, checkboxes, and drags worked in tests but not in the live
  app.**
- **A stale periodic scan could clobber a freshly created task.**
- **Test runs no longer write to the real `~/.switchbard/config.toml`.**
- **`GIT_*` discovery variables are scrubbed on every git invocation**, so an
  inherited environment cannot silently point a command at the wrong repo.
- **Single-instance lock is acquired before the window opens.**

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
