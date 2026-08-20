# Product Trajectory — switchbard

The doc standards Rule 1 sends agents to before non-trivial work. Build what's marked
**planned**; flag **speculative** and don't pre-build it; if the area is undocumented
and scope is ambiguous, STOP and ask the owner, then record the answer here.

## What switchbard is becoming

An **open-source desktop dashboard** — "one window for every agent, worktree, and port
on your machine." A single native egui/eframe window (no webview) that scans the OS for
listening processes, attributes each to a git worktree, reads each repo's own
declarations to predict what it *would* start, probes git state, and gives one control
surface to start/stop/kill services, open `:port`, and run the worktree lifecycle.
Local-first: no telemetry, no account, no daemon. Alpha, v0.3.0, MIT, public
(benpchandler/switchbard). The author dogfoods it daily.

## Cost-of-failure tier

**LOW** — see `power-of-10-overrides.md`. No network/PII/money/irreversible ops; the one
destructive action (worktree remove) is confirmation-gated. The dominant risk is
**public representation**: this is open-source, so legibility — clean module/domain
mapping, intent-level `//!` docs, zero-warning builds, the WCAG-AA legibility contract
(`tests/legibility_audit.rs`) — is the real bar.

## Current entry points

- **Binary:** `crates/switchbard-gui/src/main.rs` → `switchbard` (loads config, expands
  worktrees, hands to `HiveApp`). Core is library-only.
- **Debugging examples:** `probe`, `probe_services`, `classify_check`, `sweep`.
- **Backing stores:** no DB. `~/.switchbard/config.toml` (atomic write-tmp-then-rename),
  service logs in `$TMPDIR/switchbard-logs/`, perf ledger JSON in `docs/perf/runs/`,
  on-disk agent-context cache.
- **Platforms:** macOS (unsigned DMG) + Linux (build from source). CI runs both
  (macos-latest + ubuntu-latest); `release-linux.yml` ships Linux artifacts.

## Planned

- Cross-platform parity (macOS + Linux) stays a first-class, shipped invariant — keep
  `#[cfg(target_os = …)]` scanner branches in lock-step; don't regress to macOS-only.
- Worktree-first model (one repo → many worktrees) remains foundational; never collapse.
- Per-surface `Status` feedback and progressive-disclosure workspace cards continue as
  the UI direction.
- **Unified task hub (owner-approved 2026-08-04).** Switchbard becomes the single pane
  of glass for every tracked repo's Backlog.md tasks. Repos stay the system of record
  (all mutations still write through the `backlog` CLI); Switchbard is the system of
  engagement. Three slices, in order:
  1. *Unified view + global triage queue* — merge all repos into one ranked list
     (`backlog_projects_snapshot()` already aggregates); triage order overdue →
     priority → age → repo, overridable by the `ordering.yml` overlay in the
     `~/Dev/hub` repo (which also hosts repo-less tasks as a normal Backlog project).
  2. *Dispatch worker* — a fifth worker thread: user flags a task for dispatch → create
     worktree (existing lifecycle) → `spawn_in_session` headless `claude -p` → on
     success `gh pr create` → append PR link to the task's notes.
  3. *Headless dispatcher binary* — thin `switchbard-dispatch` bin reusing
     `switchbard-core`, run by launchd, drains the dispatch queue with the GUI closed.
  4. *Dispatch view* (owner-approved 2026-08-06) — a fourth `ViewTab` (nav label
     "Dispatches") listing every dispatch-labeled task across all repos, grouped
     attention-first: **finished-never-released** → in flight → queued → failed →
     awaiting review. Shows elapsed time, branch, worktree, log path, PR link,
     failure reason. The `dispatching` label alone is *not* treated as proof a run
     is live — an orphaned run wears it forever — so the view cross-checks it
     against `DispatchRun::looks_orphaned` (agent output written, `RELEASE_GRACE`
     elapsed, claim still held). Found by dogfooding: MusicProduction TASK-307 sat
     under "In flight" for 84 minutes after its agent had already finished and
     committed. See TASK-39 for actually recovering such runs. **No run store**:
     `dispatch_inspect` rebuilds every path from repo root + task id, and recovers
     the start time from the unix stamp already embedded in the log filename, so
     inspection survives an app restart and `workers::spawn_dispatch` keeps its
     "publishes no state of its own" property. The `dispatch_runs` map on `HiveApp`
     is a *cache* refreshed by the backlog worker purely to keep `read_dir` off the
     render path — the labels stay authoritative.

- **Refine — AI-assisted grooming, upstream of dispatch (owner-approved 2026-08-19).**
  A "Refine" button in the task detail rail, next to Dispatch. It feeds the task's
  current title/description/criteria/plan to a headless `claude -p` run at the repo
  root (no worktree — it writes no code) under a read-only permission posture
  (`--permission-mode plan`, a Read/Grep/Glob allowlist, an explicit
  Bash/Write/Edit/WebFetch/WebSearch deny list, and a turn cap), takes back one
  strict JSON object, and applies it **additively** through the same `backlog` CLI
  path every other mutation uses: the original description survives verbatim as a
  prefix, existing acceptance criteria keep their text *and* checked state (new ones
  are appended via `--ac`), an empty plan is filled and a non-empty one extended.
  Malformed or partial output applies nothing — parsing and merging both complete
  before the single `backlog task edit`. `switchbard_core::refine` owns the contract;
  see its module doc.
  - *Why it exists:* a half-baked card dispatched as-is produces a weak agent run.
    Refine is the grooming step that makes a card dispatch-ready; Dispatch is
    unchanged and still strictly opt-in.
  - *Deliberately no new label state machine.* Dispatch's `dispatch`/`dispatching`/…
    labels guard a long PR-opening pipeline from running twice. A refine run is one
    bounded call with an additive-only effect, so the "don't stack runs" guard is an
    in-memory set on `HiveApp` (`refining_tasks`), not state written into the repo.
  - *"Verbatim" is a file-level claim, and it took a parser fix to make it true.*
    `backlog::parse::extract_section` was lossy in the read direction — it ended a
    section at any `## ` line, including one inside a code fence, and dropped every
    `<!-- … -->` line — while every replace-write in the app (`-d`/`--plan`: the
    detail rail's Save as well as Refine) writes back what that reader returned. So a
    fenced heading, a hand-written HTML comment, or anything after such a fence was
    deleted on the next save. The reader is now fence- and comment-aware, and any
    replace-write is additionally gated on `task_file_round_trips`: if the parser
    cannot reproduce the file's own content, the description and plan are skipped
    (the criteria append still runs — `--ac` adds to a list rather than replacing a
    section) and the status line says why. Unknown future lossy cases degrade to a
    visible no-op, never a silent deletion. The one remaining qualification is the
    CLI's own collapsing of blank runs, which it applies to every write regardless.
  - *Accepted risk, named not mitigated — write amplification into dispatch.* Refine
    output persists into a task's description and acceptance criteria; those are
    exactly the fields `dispatch::build_dispatch_prompt` later embeds verbatim into a
    run under `--permission-mode acceptEdits`. So text one model wrote can become the
    instructions another model executes with edit rights. The gate is human review of
    the marked "Refined by Switchbard" block before anyone flags the task for
    dispatch — which is why the block is visibly marked rather than blended in, and
    why Refine and Dispatch stayed two separate opt-in clicks. No further mitigation
    is being built now; revisit if refine output ever reaches dispatch without a
    human in between.
  - **Speculative, do NOT pre-build:** batch refine (refine a filtered set / a whole
    column) and auto-refine-on-dispatch (a thin card silently refined before its
    dispatch run). Both are plausible; neither is approved. Auto-refine-on-dispatch
    in particular would remove the human gate named just above, so it is not a pure
    convenience change. Ask the owner before building either.

- **Standardized cross-repo status vocabulary (owner decision 2026-08-06).** Every
  tracked project offers the same statuses — `Icebox → To Do → In Progress →
  In Review → Done` (`switchbard_core::STANDARD_STATUSES`) — regardless of what its
  own `backlog/config.yml` declares. Chosen from evidence, not invented: a survey of
  all 8 configured repos found `budget` already declaring exactly this list and the
  other four backlog-bearing repos declaring a strict subset, so 322 of 323 existing
  tasks already conformed. `ordered_status_vocabulary` seeds from this set rather
  than the narrower `BACKLOG_STATUSES` trio, which is what makes the guarantee
  scope-independent (the detail rail passes a single project).
  - Accepted cost: a project with no `Icebox` tasks still renders an empty `Icebox`
    Board column. `board_shows_the_full_standard_vocabulary_even_when_a_project_
    declares_none` is the deliberate reversal of the older test that asserted the
    opposite.
  - The one non-conforming task (a single MusicProduction task on `Backlog`) is
    intentionally left alone; `CANONICAL_STATUS_ORDER` still sorts it sensibly.
  - **Not yet done:** the other repos' `backlog/config.yml` files are unchanged.
    Switchbard offers the standard set regardless, so this is cosmetic — but until
    those are rewritten, `backlog` CLI users outside Switchbard still see each
    repo's old declared list.

- **Dispatch lifecycle status transitions (owner decision 2026-08-06).** Claiming a
  task sets its status to `In Progress`; opening its PR sets `In Review`. A failed
  run *restores the status the task carried before the claim* rather than picking a
  fixed default, so the pipeline is a true inverse of itself. No hook or callback is
  involved — `dispatch_one` blocks on `wait_for_exit` and already knows the outcome.
  The label state machine remains the authority on pipeline state; the status exists
  so an agent-worked task is visible to someone reading the board rather than the
  dispatch pill.

## Speculative (do NOT pre-build)

- Windows support. (No `cfg` branches, no CI, no demand recorded.)
- Signed/notarized macOS distribution.
- Any daemon, account, sync, or telemetry — explicitly against the local-first stance.
  Owner-scoped exception (2026-08-04): the launchd-run `switchbard-dispatch` binary
  (Planned, slice 3) is a scheduled local process, not a resident daemon, account, or
  network sync — the local-first boundary otherwise stands unchanged.
- Plugin/extension surface for custom service detectors.

Flag any of these the moment a task seems to assume it, and confirm with the owner
before building.

## Known gaps / debt

- **Oversized UI files (Rule 4/6 debt):** `ui/workspace/mod.rs` (~1778 LOC) and
  `ui/backlog.rs` (~1710 LOC) run against the repo's small-module ethos. Split them when
  next touched; do not pile new UI onto them. (Mirrored in `power-of-10-overrides.md`.)
- **Stale README hook reference (fixed on this branch):** `README.md` §Development
  previously referenced a tracked pre-push hook (`mise run hooks:install`) removed in
  commit `9ae32e2`, and described CI as macOS-only. Both corrected here: there is no
  hook (run `mise run ci` manually before pushing), and CI runs macOS + Linux.
