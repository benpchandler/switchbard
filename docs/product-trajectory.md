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
  5. *Dispatch operability* (TASK-43, owner-approved 2026-08-19) — dispatch stops
     being pull-only. An always-visible top-bar chip (`⚙ N running · <oldest
     elapsed>`, in `theme::dispatch_accent`) says an agent is working from any tab,
     flipping to danger styling **and different wording** when anything is failed,
     orphaned, or past its advisory staleness threshold (TASK-46 below — the run
     itself keeps going either way); clicking it goes to the Dispatches tab, which
     also carries a badge counting runs (not queue depth). Silent when there is
     nothing to say — the same "no ticking counters" rule as the removed last-scan
     label. And a hand on the plug: `dispatch_one` writes a **pgid sidecar**
     (`dispatch-<task>-<stamp>.pid`, beside the run's log) between spawning the
     agent and blocking on it, deletes it as the run releases, and the Dispatch
     view's in-flight rows get a confirm-armed Kill button (identity-gated — see
     below; no automatic deadline attached to it, see TASK-46).
     - The sidecar is **not a run store**: it is the one fact about a run that
       cannot be rebuilt from repo root + task id (the kernel assigns the pgid), it
       is named by the same stem convention as the log, and nothing reads it as
       authority on pipeline state.
     - The kill needs **no coordination with the worker thread**. `dispatch_one` is
       already blocked in `wait_for_exit` on that process, so one signal makes the
       wait return and the existing path releases the task as `dispatch-failed`
       with a note. Writing state from the UI would be a second writer racing the
       first.
     - `DispatchOptions::max_turns` (default 50) is passed as `claude -p
       --max-turns`, bounding *looping* from inside the agent. At the time this
       bullet was written it was complementary to a wall-clock `timeout` that
       bounded *hanging* from outside; TASK-46 (below) removed that timeout, so
       `max_turns` is now the only automatic bound left on a runaway run.
     - **The sidecar is self-authenticating** (adversarial review, 2026-08-19).
       A bare pgid on disk is a loaded weapon: force-quit Switchbard mid-run and
       the file survives, macOS recycles pids at 99999, and the Kill button then
       aims at whatever inherited that number — under a dialog reassuring the
       user it is safe. So the sidecar is versioned and records the boot epoch
       (`switchbard_core::boot_time`), the supervisor's pid, and the run's start
       stamp, and `dispatch_inspect` issues a `DispatchRunLiveness` verdict that
       **fails closed**: a kill handle exists only when the sidecar was minted
       this boot *and* a live process in that group still carries this run's own
       prompt path. Legacy bare-pgid files, other-boot files, and failed probes
       are all `Unverifiable` — no button, with the reason shown in its place.
     - **Supervision is a first-class distinction.** When the Switchbard that
       spawned an agent is gone, no `wait_for_exit` is watching it and nothing
       will release the task when it ends. Such a row — if the agent is still
       identifiable — offers a Kill labelled honestly ("task stays on
       `dispatching`"). A verified-**dead** group under a still-claimed task
       classifies as needs-attention rather than sitting under "In flight"
       forever; that is the case file evidence alone cannot see, because an
       empty log is also what a healthy run looks like. (At the time this was
       written, supervision also gated a `hard kill in Ym` deadline label;
       TASK-46 removed that label along with the deadline it described.)
     - **Claiming clears the previous attempt's terminal labels**, and a live
       `dispatching` claim outranks a stale terminal verdict in the ladder.
       Previously the ladder ran `dispatched` > `dispatch-failed` >
       `dispatching`, so a re-flagged task reported Failed for the whole length
       of its new run and lit the attention chip with a warning nothing could
       clear. `claim_task_for_dispatch` strips the stale labels; the reordered
       ladder is the fallback for when that best-effort strip fails.
     - **A cached verdict may render a button; only a fresh one may fire it.**
       The liveness verdict on a `DispatchRun` is up to ~4 minutes stale (30s
       worker cadence × the unfocused backoff), and in that window a pgid can
       be reissued. `dispatch_kill::kill_dispatch_run` therefore re-runs the
       *same* authenticated probe on the thread that signals, immediately
       before signalling, and refuses ("run already ended — nothing killed")
       rather than firing at a number it can no longer vouch for. The two
       answers it can give are "killed it" and "nothing killed" — never
       "killed something".

  6. *Drop the strict wall-clock kill — advisory staleness only* (TASK-46,
     owner-approved 2026-08-20, LED-580 post-mortem). A genuinely productive
     30-minute run — 29 files of real work — was hard-killed the instant it
     crossed `opts.timeout`, stranding everything it had not yet committed. No
     code path kills a dispatch run on wall-clock time any more:
     `DispatchOptions::timeout` is renamed `stale_after` and is now purely
     advisory (`crate::dispatch::DEFAULT_STALE_AFTER`, still 30 minutes —
     the number was never wrong as a "go check on this" signal, only as a
     kill trigger). `run_claude_headless`'s wait is, for all practical
     purposes, unbounded. What still bounds a runaway run: `--max-turns`
     (default 50, unchanged) from inside the agent, and the identity-gated
     Kill button for a human watching a run go wrong right now — both
     pre-existing, neither weakened.
     - A run past `stale_after` still counts as *running* in the chip and the
       Dispatch view — needs-attention, not failed, not killed. Copy changed
       to match: "running 47m — check on it" replaces "hard kill in Ym"; the
       deadline label and the hover text promising a kill are gone entirely,
       for both supervised and unsupervised runs.
     - Accepted consequence: since `drain_dispatch_queue` processes its batch
       serially (see that function's own "why serial" doc) and a run's wait
       is no longer wall-clock bounded, one long run now delays every other
       task queued behind it in the same drain call — and the GUI's dispatch
       worker thread (`workers::spawn_dispatch`) blocks for exactly as long.
       There is no wall clock left to cap that delay. Judged acceptable
       because dispatch is opt-in, low-volume, and `max_concurrent` already
       caps a single drain's batch size.

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
    deleted on the next save. The reader is now fence-aware (with CommonMark's
    closer-length rule) and drops only the CLI's own markers, and Refine's
    replace-writes are additionally gated on `task_file_round_trips`: if the file's
    structure isn't one a section-replace can be based on, the description and plan
    are skipped (the criteria append still runs — `--ac` adds to a list rather than
    replacing a section) and the status line says why.
    - *What the guard is and isn't.* It requires balanced fences, and every `## `
      heading to be one of the six the format defines and appear once, before it
      compares content line by line. Those structural rules exist because the first
      version checked conservation alone and was **circular** — it derived "which
      lines are headings" with the same predicate the reader used, so a lossy read
      that surfaced as a spurious heading was self-consistent and passed. It now
      bounds that class; it is a strong check, not a proof of losslessness.
    - *It fires on real data.* Across 345 real task files in three repos, 51 fail —
      every one because it carries a human-written section the format has no field
      for (`## Resolution`, `## Root Cause Hypothesis`, `## Reproduction Steps`).
      `parse_task_file` extracts six sections; content under any other heading lands
      in no field at all, so a replace-write really would delete it.
    - **Residual (pre-existing, unfixed): the detail rail's Save does not consult
      the guard.** It writes `-d` from the same parsed description, so on exactly
      those 51 files it can still delete a custom section. Refine is guarded; Save
      is not. Fix it when Save is next touched.
    - Whitespace qualification: every non-blank line of the original survives in
      order, byte for byte. Blank runs collapse to one (the CLI does this to every
      write regardless) and whitespace-only lines lose their whitespace.
  - *Residual, unfixable from here: hooks are not covered by the permission flags.*
    `--permission-mode plan` and the tool deny list constrain the model's tool use,
    not Claude Code's own hook machinery, which the **target** repo configures in its
    `.claude/settings.json`. Refining a repo means trusting that repo's hooks —
    "read-only" is a statement about the agent, not a sandbox.
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
