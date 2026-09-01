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

> **Note (feat/landing-stage):** "No network ops" above is stale and getting staler.
> `dispatch` already pushes and opens PRs via `gh`; the landing-stage worker
> (`switchbard-core/src/landing.rs::probe_pr_state`, `switchbard-gui/src/workers.rs::
> spawn_landing`) adds a second capability — Switchbard now *reads* GitHub (`gh pr list`)
> to explain why a worktree's unlanded commits are still unlanded. Still read-only, still
> opt-in-by-failure-mode (a missing/unauthenticated `gh` degrades to an explicit "unknown"
> rather than blocking anything), and still LOW by this tier's own criteria (no PII/money/
> irreversible ops) — but the tier line's literal wording no longer matches what the app
> does, and should be corrected the next time this section is substantially revised.

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
- **Agents context + hooks (owner-approved 2026-08-28).** The former `Agent Context`
  top-level tab is `Agents`, with sibling `Context` and `Hooks` surfaces. Context keeps
  the existing file explorer. Hooks parses Claude user, shared-project, and
  local-project settings and leads with an inferred plain-English purpose and trigger,
  followed by event, matcher, handler type, action, arguments, condition, timeout, scope,
  and source for the selected repo. Unsupported matcher/condition combinations are called
  out instead of being presented as runnable. Hook arrays merge across
  settings levels, exact duplicate registrations render once, and `disableAllHooks`
  follows user < project < local precedence so configured-but-disabled hooks are not
  presented as active. This remains a best-effort local settings scan, not session-state
  proof: managed policy, plugin-provided hooks, and skill/subagent session hooks are not
  yet visible.
- **Shared filter controls (owner-approved 2026-08-28).** Search fields, facet labels,
  wrapping filter containers, active-count treatment, and clear/reset behavior live in
  `ui/filter_bar.rs`. Agents, Workspace, the top-level view filter, and Backlog consume
  those primitives while retaining domain-specific predicates and state. Agents places
  the shared bar directly below Context/Hooks: Context exposes agent, scope, and asset
  type; Hooks exposes agent, scope, event, and handler type. Text search must narrow the
  individual rows/cards it claims to match, not merely leave an entire repository card
  visible because one hidden child matched.
  Last-used queries and ordinary facets persist per surface in `UiConfig.filters`; adding
  another filter surface uses the same query-plus-named-facets record rather than adding
  page-specific config fields. Confirmation state, bulk selections, open editors, and
  in-flight actions remain session-only.
- **Unified task hub (owner-approved 2026-08-04).** Switchbard becomes the single pane
  of glass for every tracked repo's Backlog.md tasks. Repos stay the system of record;
  Switchbard is the system of engagement. (The "all mutations still write through the
  `backlog` CLI" half of the original decision is superseded by the *Backlog format
  fork* entry below, 2026-08-28 — repos-as-system-of-record is unchanged.) Three
  slices, in order:
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

- **Backlog format fork (owner-approved 2026-08-28).** Switchbard forks the Backlog.md
  task format at its current on-disk shape and becomes its owning tool. The 2026-08-04
  "all mutations write through the `backlog` CLI" decision is superseded: the external
  CLI and MCP writers are deprecated for tracked repos, because keeping them alongside
  native writes would mean two writer *implementations* for the same files — exactly
  the one-fact-two-sources shape this project keeps re-learning. The invariant that
  replaces it: **one writer implementation** (`switchbard-core`'s write layer), many
  frontends (GUI, `switchbard-dispatch`, a thin `switchbard task` CLI for terminals
  and agents). Files stay where they are and stay readable by anything that speaks
  Backlog.md until a divergence task explicitly says otherwise. Sequencing
  (TASK-62…68, each gated on its predecessor):
  1. *This decision record* (TASK-62).
  2. *Native write layer* (TASK-63) — surgical, atomic task-file mutations in
     `backlog::write`; byte-preservation gate over every real task file; no-op edits
     write nothing; body edits fail closed on `!body_round_trips`.
  3. *Native ID allocation* (TASK-64) — replaces the CLI's `check_active_branches`
     scan, worktree-aware because switchbard already enumerates worktrees.
  4. *The swap* (TASK-65) — `mutations.rs` reimplemented on the write layer behind
     unchanged signatures; this is when module docs here and in `dispatch`/`refine`
     that name the CLI as write authority get rewritten, and when
     `parse_created_task_id` stdout scraping dies.
  5. *Thin CLI frontend* (TASK-66) — preserves dispatch's "flaggable from a plain
     terminal" property and gives agents their write path.
  6. *Deprecation* (TASK-67) — CLI probing, degraded modes, and the mise pin removed.
  7. *Divergence on named wins only* (TASK-68) — parent-key collapse, dispatch state
     as a first-class field, status validation in the write layer. Compatibility is
     no longer owed, but every divergence must still name its win and land in this
     doc.

- **Linear-vocabulary hierarchy (owner-approved 2026-08-31; named win per TASK-68).**
  Four tiers, Linear's model and language: Initiative - Project - Issue (task) -
  Sub-issue (decimal child). The win: one vocabulary from disk to UI - the
  `milestone:` frontmatter key never meant milestones here, and every surface
  renamed around it would otherwise sit on a lying storage key forever. "Repo"
  replaces "project" for the repo-backlog scope everywhere user-facing (Linear's
  Team analog); `--project <DIR>` survives as a deprecated alias of `--repo <DIR>`.
  The divergences, each named:
  1. *Task membership key is `project:`.* `milestone:` is read as a legacy fallback
     (both present: warning, `project:` wins) and rewritten in place only when
     membership itself is written - no mass migration; unrelated edits stay
     byte-surgical, so untouched files remain byte-identical and upstream-readable.
  2. *Optional definition files* `backlog/projects/<slug>.md` and
     `backlog/initiatives/<slug>.md` (frontmatter `name`/`status`/`target_date`/
     `initiative`/`lead` + markdown description) enrich name-keyed groups with
     lifecycle. A project exists if any task references it *or* a def declares it;
     defined-but-empty renders 0/0, which is what makes `project create` before
     task assignment meaningful.
  3. *Project lifecycle statuses* (`Planned/In Progress/Completed/Canceled`,
     `PROJECT_STATUSES`) are a separate vocabulary from task statuses, validated
     only on def writes. Tasks may reference undefined projects - that is how
     projects are born, mirroring how milestones were never validated.
  4. *Roll-up is computed, never stored* (`compute_hierarchy_rollup`): per-project
     and per-initiative done/total, cross-repo by exact name match; def-name
     conflicts resolve to the alphabetically-first repo, deterministically.
  Deliberately deferred, ask before building: `project rename` (a bulk task-file
  mutation), GUI def-file authoring (def lifecycle is CLI-first in v1),
  case-insensitive name merging.

- **Weekly goals (owner-approved 2026-08-31; TASK-70..74).** Weekly numeric
  goals tracked relative to target ("onboard 5 users this week" vs 4 actually
  onboarded). Two goal kinds: *task-derived* (actual computed from tasks done
  within the goal week matching a scope - a project name or label - and/or
  the goal's attached inputs) and
  *manual-metric* (actual reported as dated, append-only check-in
  observations; "current" derives from the latest entry).
  - *Storage is `backlog/goals.yml`, one structured file per repo - NOT
    markdown def files* (owner decision after review of the markdown design):
    goals are records, not documents. A goal's shape is name/unit/measure/
    scope plus a `weeks` map of `{target, checkins: [{date, value}]}`, so
    cross-week history is one read and `goal roll` adds a week key rather
    than cloning files. Writes are line-surgical YAML edits through the
    shared write layer (precedent: `status_config.rs` on `config.yml`);
    reads are tolerant - a malformed file warns and loads empty, never
    failing the repo load. Goals ride `load_backlog_repo`, so they reach
    every snapshot with no new IO or worker.
  - *Pace is the load-bearing derived signal, computed never stored:*
    `actual/target` vs `elapsed_days/7` yields on-track / behind / met /
    missed; met and missed are the terminal verdicts at the week boundary.
  - *Input goals (owner requirement 2026-09-01; TASK-92).* Tasks and
    projects can be ATTACHED to a tasks-measured goal as counted inputs
    (`inputs: {tasks, projects}` in `goals.yml`; `goal attach` /
    `goal detach`, line-surgical like every other goals write). The actual
    counts a task once if it matches the scope OR is an attached task OR
    belongs to an attached project; a scopeless tasks goal is legal and
    counts only its inputs. Attach canonicalizes task ids against the
    backlog; manual goals refuse inputs (they take check-ins).
  - Surfaces: a `goal` CLI verb family (create / list / view / check-in /
    roll / attach / detach) mirroring the `project` family's output contract, and a "This
    week's goals" section leading the Digest lens (progress bar with a
    today-tick at the elapsed-week fraction, pace pill, check-in affordance
    for manual goals).
  - **Speculative, do NOT pre-build:** auto-recurring goal templates - v1's
    `goal roll` (explicitly cloning last week's targets into a new week key)
    is the recurrence story until demand says otherwise.

- **Stack ranking (owner-approved 2026-08-31; Stack Ranking project).** Manual
  stack rank within a repo: hierarchy-shaped, with a named exception lane.
  - *Rank siblings within their parent scope* - projects against projects
    within the repo, tasks within their project (repo-root tasks form one
    sibling group), sub-issues within their parent task. Ranking is sparse:
    only explicitly ranked items float, in rank order, above the unranked
    rest, which keeps sorting by the existing computed comparator
    (status - priority - id). A neglected rank list degrades gracefully
    instead of lying.
  - *The repo-wide "next up" order is computed, never stored* (roll-up
    discipline): expedite lane first, then a top-down flatten - top-ranked
    project's task stack, then the next project's, then unranked projects.
  - *Expedite lane:* a short explicit list of task ids that jump the entire
    computed order - true cross-project interrupts only. Owner insight
    shaping v1: most historical queue-jumping was an *incomplete queue*,
    not an emergency, so `create` takes rank-placement flags (top /
    before / after a sibling) and a newly discovered task lands properly
    ranked among its siblings instead of reflexively expedited. When the
    interrupt ships, it leaves the lane and the hierarchy is untouched.
  - *Storage is `backlog/ranking.yml`*, one records file per repo owned by
    `backlog/ranking.rs` (goals.yml precedent: records, not documents;
    never hand-edit). Named `ranking`, not `ordering`, because the hub
    repo's root-level `ordering.yml` (the cross-repo triage overlay,
    `OrderingOverlay`) already exists as a different authority - two files
    sharing one name would be a findability trap. Line-surgical writes
    through the shared write layer; tolerant reads (malformed warns and
    loads empty). Rank does NOT live in task frontmatter - inserting at a
    position would mass-rewrite every file below it, breaking the
    byte-surgical discipline - and the old CLI's `ordinal` stays
    unwritten. Entries naming done/archived/missing ids are ignored on
    read and pruned on the next write to their scope.
  - Surfaces (one slice, owner-chosen): a `rank` CLI verb family
    (`rank project <name>` / `rank task <id>` with `--top`/`--before`/
    `--after`, plus `unrank`), `expedite`/`unexpedite`, create-time
    placement flags; `list` and `project list` output honors the order.
    GUI backlog surfaces sort by the computed order and get reorder
    controls (move up/down on project and task rows, expedite toggle) -
    run `design-state` before building the controls.
  - This is the per-repo half of the unified-hub triage overlay (slice 1
    above): the hub's cross-repo queue composes per-repo orders later.
  - **Speculative, do NOT pre-build:** initiative-level ranking, the
    cross-repo ordering overlay itself, drag-and-drop reorder (move
    up/down buttons are v1 unless the design-state pass says otherwise).

- **Task Queue orchestration (owner-directed 2026-09-01; TASK-88..91).** The
  owner's goal statement, verbatim intent: users send tasks to the queue,
  an orchestration agent picks them up for dispatch **using LangGraph** and
  monitors them to completion; tasks can be reordered; live progress is
  visible. Decisions:
  - *The queue is not a new store.* It remains the dispatch-labeled task
    set, now ordered by the stack rank's computed flatten (which
    `list_dispatch_queue` gets for free since ranking landed in
    `load_backlog_repo`). Reordering the queue = re-ranking; that AC is
    already true by construction and gets proven end-to-end in TASK-91.
  - *The protocol surface is a `queue` verb family on `sb` (the switchbard-task crate)*
    (TASK-88), designed against `~/.claude/standards/agent-facing-design.md`:
    claim is the acknowledgment (the existing `dispatch` -> `dispatching`
    label swap, before any work), releases carry outcome + note through the
    native write layer, and `queue prompt` exposes `build_dispatch_prompt`
    so the orchestrator never re-derives the prompt. The orchestrator only
    ever mutates tasks through this CLI - the one-writer invariant extends
    to it.
  - *The orchestrator is Python + LangGraph at `orchestrator/`* (TASK-89),
    adopting the substrate the xplan `langgraph-mission-shadow` probe
    validated (durable SQLite-checkpointed StateGraph, interrupt with the
    exact remainder, restart-safe resume). Its per-task graph is
    claim -> worktree -> agent run (headless `claude -p`, `acceptEdits`,
    turn-bound, no wall-clock kill - the LED-580 lesson stands) ->
    gate -> reconcile -> PR -> release. *Reconcile* carries the shadow's
    completion-integrity model into production shape: every AC must map to
    evidence; task-green without outcome-proof interrupts rather than
    releasing a false `dispatched`. uv-managed, pinned deps; not in
    `mise run ci` v1 (its own gate lands with the crate when it stabilizes).
  - *Live progress* (TASK-90) is an append-only JSONL events sidecar next
    to the run log (`<stem>.events.jsonl`) that `dispatch_inspect` folds
    into `DispatchRun`; the Dispatches view renders current phase and
    heartbeat age. Missing/malformed sidecar degrades to today's view.
  - The existing Rust `switchbard-dispatch` drain stays as the fallback
    path until the orchestrator has dogfood proof (TASK-91); GitHub
    delivery-awareness (TASK-80.x) sequences after this, per the owner's
    rank flip.

- **Information architecture V2 - places and objects (owner-approved 2026-09-01;
  TASK-76 mockup over 11 owner review rounds; this entry is TASK-77's decision
  record).** Navigation reorganizes from surface types (seven sibling Backlog
  lenses) to a sidebar of *places* over the object model: **Digest, Tasks,
  Command, Goals, Ops**, scoped by a **multi-select repo switcher** - check any
  set of tracked repos and every place aggregates over the selection. Frozen
  visual reference: the TASK-76 Lavish artifact (both real theme palettes,
  `~/.lavish/switchbard-ia-places.html`).
  - *Tasks is the primary work list; "project" is a grouping, not a place.*
    Group-by is generic over every available field (project, status,
    initiative, priority, label, repo, ...) - never hardcoded options;
    filtering is a filter-builder plus recent filters, no hardcoded chips.
    The standalone project page is CUT: a group header expands in place for
    its summary (computed roll-up, goal pace, description), and the stack
    rank surfaces only as a sort option, never a page or dedicated column.
    Repo badges sit on rows, always. Sub-issues indent in place, expanded.
  - *Dispatch has two axes.* A built-in "Dispatches" view under Tasks is the
    task-delivery facet (run status, kill/retry/log per row); **Command** is
    the agent-scoped fleet console as its own place - agents, missions,
    worktree leases, live activity lines, SITREP age, and support requests
    (NEEDS_DECISION and kin) with respond affordances and blast-radius notes.
    The sidebar footer lamp stays ambient status and deep-links.
  - *Digest leads with goal cards, then in-flight work, then the attention
    feed* - feed rows are computed from their owning objects (PR probe, run
    reaper, server watch, port scan, `removal_safety`), deep-link there, and
    reuse those surfaces' command verbs with inline icon actions; nothing is
    stored on tasks.
  - *Ops (renamed from "Repos" so repo only ever names the scope)* keeps the
    entire Servers/Workspace toolset, fully merged to one row per worktree:
    detected services with start, running services with stop / open-in-browser
    / logs, listeners and external squatters with kill, git state, agent
    sessions attributed per worktree, and removal behind the `removal_safety`
    verdict.
  - *Sidebar follows the Linear pattern:* a FAVORITES group at the top holds
    explicitly favorited objects rendered with their type glyphs (no pin
    icons; nothing auto-populates); saved filters are first-class named
    views; under Tasks live only the built-in views (All tasks, Dispatches).
  - *Implementation obligations, recorded in the mock's state matrix:*
    selection uses the Board's stroke-based ring (unifies with TASK-38);
    universal actions are icon buttons whose accessible names derive from the
    same command-verb authority that names them everywhere (AccessKit label =
    verb name); the focus ring reuses the selection stroke; task lists
    inherit TASK-13 virtualization; dark-theme chip tints stay at low alpha
    and dark `warn_orange` needs the contrast fix noted on TASK-78; goal
    pages render the Inputs card over TASK-92's attach/detach.
  - *Rejected along the way (recorded so they stay rejected):* a global
    object tree with repo badges as the primary scope model;
    one-repo-at-a-time switching; a Projects place or index page; a burndown
    chart owning a page; pin icons; hardcoded grouping or filter chips;
    text-labeled buttons for universal actions.
  - *Persisted per-surface state migrates by surface key:* `UiConfig.filters`
    records re-key from lens names to place/view names as each place lands
    (unmatched old keys are dropped, not migrated by guess); expansion
    toggles and selections stay session-only as today.
  - Implementation tasks are defined only now that this record exists
    (mockup -> decision record -> implementation, per the project
    definition). The lens code being replaced keeps working until each place
    lands; nothing here licenses a big-bang rewrite.

- **Refine — AI-assisted grooming, upstream of dispatch (owner-approved 2026-08-19).**
  A "Refine" button in the task detail rail, next to Dispatch. It feeds the task's
  current title/description/criteria/plan to a headless `claude -p` run at the repo
  root (no worktree — it writes no code) under a read-only permission posture
  (`--permission-mode plan`, a Read/Grep/Glob allowlist, an explicit
  Bash/Write/Edit/WebFetch/WebSearch deny list, and a turn cap), takes back one
  strict JSON object, and applies it **additively** through the same mutation path
  every other write uses (the `backlog` CLI then; the native write layer since the
  format fork's TASK-65 swap): the original description survives verbatim as a
  prefix, existing acceptance criteria keep their text *and* checked state (new ones
  are append-only), an empty plan is filled and a non-empty one extended.
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
    - *What the guard is and isn't.* It requires balanced fences, no `## ` heading
      to repeat, and no known section heading inside a fence, before it compares
      content line by line. Those structural rules exist because the first version
      checked conservation alone and was **circular** — it derived "which lines are
      headings" with the same predicate the reader used, so a lossy read that
      surfaced as a spurious heading was self-consistent and passed. It now bounds
      that class; it is a strong check, not a proof of losslessness.
    - *Custom sections are preserved, not refused (TASK-45, decided 2026-08-31).*
      Across 345 real task files in three repos, 51 carry a human-written section
      the format has no field for (`## Resolution`, `## Root Cause Hypothesis`,
      `## Reproduction Steps`); the guard originally treated any unknown heading as
      a reason to refuse prose writes. TASK-45 posed refuse-vs-preserve as a product
      call and it resolved to **preserve**: the native write layer (TASK-63) is
      surgical — a section edit rewrites only its own span — so an unknown heading's
      block is opaque to every operation and survives byte-for-byte, with the
      round-trip guard's conservation rule extended to cover it. Refusing would
      have frozen saves on ~15% of real tasks to defend against nothing; the
      residual cost of the preserve stance is that prose misread as a heading
      splits into its own opaque section instead of refusing — a survivable
      outcome, where the alternatives were refusal or deletion. Only duplicated
      headings and unbalanced fences still refuse.
    - *The old residual is closed.* The detail rail's Save has written through the
      same guarded write layer since the TASK-65 swap, so Refine and Save now share
      one preservation contract; the 51-file class blocks neither.
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

- **"Safe to remove" now has exactly one definition** (`switchbard-core/src/removal_safety.rs`).
  It previously had three that disagreed: the Workspace row badge ran three checks, the bulk
  sweep ran five, and the single-row confirm dialog re-derived merged-ness from
  `BranchDeleteAssessment::needs_force()`. The same worktree could read "remove ok" on the row
  and land in the sweep's "needs review" list in the same frame. Two remaining sharp edges,
  both deliberate and both documented at their sites:
  - `WorktreeMeta`'s probe fields use `None` for *both* "not probed yet" and "the probe
    failed", a convention older than this module. The Workspace row therefore shows a
    persistently failing probe as perpetually `checking…` rather than as blocked. Never a
    false green, and the surfaces that actually remove things call `probe_facts` instead,
    which distinguishes the two. Fixing it properly means moving `WorktreeMeta` onto `Fact<T>`.
  - **"Has it landed" is content-based, but only per-commit.** `unlanded_commits` uses
    `rev-list --cherry-pick`, so a rebase merge (patches preserved under new SHAs) is
    correctly detected as landed, and the base prefers `origin/main` over a stale local
    trunk. Measured on an 11-repo machine, those two together took removable worktrees from
    8 to 19. A **true multi-commit squash merge is still a false negative**: patch-ids are
    per-commit, so N commits squashed into one match none of them. It fails toward refusing
    to remove, which is the right direction, but users will hit it. Detecting it would need
    a different signal (a merged-PR lookup, or a tree comparison against the merge base).
  - **A rebase-merged branch outlives its worktree.** The work is safe to remove, but
    `git branch -d` is ancestry-based and refuses it. Rather than reach for `-D` on
    Switchbard's own authority, the sweep removes the worktree, keeps the branch, and says
    so in the status line (`branch_left_rebase_merged`). Upgrading that to `-D` on the
    strength of patch equivalence is a deliberate, un-taken decision, not an oversight.
- **Oversized UI file (Rule 4/6 debt):** `ui/workspace/mod.rs` (~1818 LOC) runs against
  the repo's small-module ethos. Split it when next touched; do not pile new UI onto it.
  (Mirrored in `power-of-10-overrides.md`.) The `ui/backlog.rs` half of this entry is
  discharged: it is now `ui/backlog/`, 19 files, largest `board.rs` at ~883 LOC.
- **Stale README hook reference (fixed on this branch):** `README.md` §Development
  previously referenced a tracked pre-push hook (`mise run hooks:install`) removed in
  commit `9ae32e2`, and described CI as macOS-only. Both corrected here: there is no
  hook (run `mise run ci` manually before pushing), and CI runs macOS + Linux.
