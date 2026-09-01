# CLAUDE.md

Guidance for Claude Code when working in this repository. Repo-specific deltas only.

**Engineering standards:** `~/.claude/shared/code-standards.md` (always loaded).
**Rust Power-of-10:** `~/.claude/standards/power-of-10/rust.md`. Repo overrides + threat tier: `power-of-10-overrides.md`.
**Where this is going / what to build:** `docs/product-trajectory.md` (read before non-trivial work — Rule 1).

## What Switchbard is

A cross-platform (macOS + Linux) local desktop dashboard — a single native egui/eframe window, no webview — that:
- Scans the OS every few seconds for listening processes (`lsof` on macOS, `/proc` on Linux), attributing each back to a git worktree by walking the process `cwd`.
- Detects what each repo *would* start by reading its own declarations: `Procfile` / `Procfile.dev`, `package.json` scripts, `Makefile`, `docker-compose.yml`, `scripts/*.sh`.
- Probes git state per worktree (dirty, ahead/behind, commit recency).
- Lets the user start a service, stop a process group, kill an external port-squatter, open `:port` in a chosen browser, and remove a worktree (`git worktree remove`, behind a confirmation dialog that enumerates uncommitted changes and tracked services).

Config is persisted at `~/.switchbard/config.toml`. Service logs land in `$TMPDIR/switchbard-logs/`.

## Common commands

```sh
mise install                          # install pinned Rust (1.95.0) from mise.toml
mise run ci                           # fmt + clippy(-D warnings) + test, same as CI
mise run bundle                       # macOS: Switchbard.app in target/release
mise run package                      # macOS: DMG + sha256 in target/dist
mise run test                         # full test suite (~0.1s)
cargo test -p switchbard-core <pat>   # single test by name substring
```

Prefer plain Cargo? Each `mise` task maps to the obvious `cargo fmt` / `cargo clippy` / `cargo test` / `cargo build --release`.

`mise run bundle` and `mise run package` require `XPLAN_SIDECAR_SOURCE` (a clean checkout of the pinned xplan revision from `xplan-sidecar-pin.json`) and `XPLAN_SIDECAR_ARCHIVE` (the sidecar archive built from it with `scripts/build_mission_sidecar.py`); the sidecar is packaged from those exact local inputs, never downloaded. Run `mise run bundle` without them for the full recipe.

## Gates (firm — CI fails on any)

CI (`.github/workflows/ci.yml`) runs the `fmt`, `clippy`, and `test` mise tasks on both **macos-latest and ubuntu-latest** on every PR. The `clippy` and `test` tasks set `RUSTFLAGS=-D warnings`, so **any compiler warning fails the build — fix it, don't `#[allow]` it.** Run `mise run ci` green before pushing.

## Render-path perf

When touching egui render paths (`crates/switchbard-gui/src/app.rs` or `crates/switchbard-gui/src/ui/**`), run a perf smoke before calling the work done: `SWITCHBARD_PERF=1` with `SWITCHBARD_PERF_LOG=/tmp/switchbard-perf.csv`, exercise Servers scrolling, compare p95 frame/workspace time against the previous build. Avoid full snapshot rebuilds, per-row clones, and unbounded per-frame file lists. Perf-ledger discipline: `docs/perf/README.md`.

## Architecture

Four-crate Cargo workspace, plus one Python package: `orchestrator/` is the
LangGraph orchestration agent (uv-managed, its own pytest suite - `cd
orchestrator && uv run pytest`; not part of `mise run ci` yet). It drains the
dispatch queue via the `sb queue` protocol and never edits task
files - see `orchestrator/README.md` and the trajectory's *Task Queue
orchestration* entry. `switchbard-core` has **zero UI dependencies** and is heavily unit-tested; `switchbard-gui` is the only place egui appears; `switchbard-dispatch` is a thin headless binary over `switchbard-core` that drains the dispatch queue with the GUI closed; `switchbard-task` (installed binary: `sb`) is the terminal/agent frontend for Backlog-format tasks over the same native write layer (format fork, TASK-66).

Mission Command uses one bundled xplan one-shot helper process per request. Switchbard may supervise only the strict `hello`, `queue_mission`, `get_pending_decision`, and `resume_decision` protocol through `switchbard-core`; xplan owns every mission write, while the egui layer renders cached state and emits typed intentions without process or filesystem I/O.

**Managing this repo's backlog tasks:** use `sb` (or `cargo run -q -p switchbard-task --` for the workspace build) (`list [--in-project <NAME>]`, `view <id>`, `create`, `edit <id> --check-ac N / --append-notes / --final-summary / -s Done / -m <PROJECT>`, `archive`, `complete`, plus the `project` and `initiative` definition families) — its `--help` is the output contract. The repo-root flag is `--repo <DIR>` (`--project <DIR>` survives as a deprecated alias). The `queue` verb family (`queue list/send/withdraw/claim/release/prompt`, TASK-88) is the orchestrator protocol: dispatch work is teed up, claimed, and released through these verbs, never by mutating task state directly. It writes through the same `switchbard-core` layer as the GUI, which is the fork's one-writer invariant. The external `backlog` CLI is retired here (TASK-67): nothing in this repo invokes it, it is not on the toolchain, and task mutations must go through `sb` (or the GUI) — never hand-edit task markdown or the `backlog/projects/` / `backlog/initiatives/` definition files.

**Hierarchy (Linear vocabulary, trajectory: *Linear-vocabulary hierarchy*):** Initiative → Project → Issue (task) → Sub-issue (decimal child). Task membership is the `project:` frontmatter key (legacy `milestone:` reads as a fallback and migrates on the next assignment); `backlog/hierarchy.rs` owns the optional def files; roll-up is computed (`compute_hierarchy_rollup`), never stored. "Repo" is the word for the repo-backlog scope everywhere user-facing.

**Weekly goals (trajectory: *Weekly goals*):** numeric targets tracked relative to the week clock, stored in `backlog/goals.yml` (records, not documents — `backlog/goals.rs` owns it; never hand-edit). Actuals come from append-only dated check-ins (manual) or done-in-week tasks matching a scope and/or attached inputs (`measure: tasks`; `goal attach` links tasks/projects as inputs); pace (`compute_goal_statuses`: on-track / behind / met / missed) is computed, never stored. CLI: `goal create/check-in/list/view/roll/attach/detach`; the Digest lens leads with the current week's goal cards.

### `crates/switchbard-core` — domain layer

Re-exports are **explicit in `src/lib.rs`** (no glob re-exports). Module map:

- `scanner` — `scan_listeners()`: per-OS snapshot of `LocalListener` rows (`lsof` / `/proc` behind `cfg`).
- `attribution` — joins listeners to `WorktreeRef`s by longest-prefix match on `cwd`.
- `worktree` — `enumerate_worktrees()` shells out to `git worktree list`.
- `worktree_remove` — dirty-file collection + `remove_worktree()`. The only destructive git op in core.
- `removal_safety` — **the** definition of "safe to remove a worktree": five named checks, tri-state facts, one verdict. The Workspace row badge, the bulk sweep, and the single-row confirm dialog all evaluate this and nothing else. Only `RemovalVerdict::Safe` may be acted on without an explicit force gesture; an unanswered check never counts as a passed one.
- `worktree_create` — `validate_refish()` rejects empty / whitespace / leading-dash refish before `git worktree add` (the repo's one true untrusted-input boundary — Rule 5).
- `workflow` — `detect_services()`: parses Procfile/package.json/Makefile/compose/scripts into `DetectedService`.
- `classify` — heuristic `Server` / `Maybe` / `NotServer` verdict per entry point.
- `expected_port`, `resolve` — port inference; clusters listeners + services into `ResolvedService`.
- `dispatch` — headless `claude -p` pipeline: dispatch-labeled task → worktree → agent run → PR.
- `refine` — the grooming step upstream of `dispatch`: a read-only headless run that fills a task's description/ACs/plan, applied additively through the native write layer.
- `git_probe` — read-only `git status` / ahead-behind / fetch age / recent commits.
- `git_env` — `git_cmd()`: every git call goes through it; see Git safety below.
- `spawn` / `kill` — `spawn_in_session()` (own session/process group) + `kill_pgid()` → `KillOutcome`.
- `config` — `~/.switchbard/config.toml` load/save; persisted form is `Vec<Repo>` + UI defaults.

### `crates/switchbard-gui` — egui/eframe app

`src/main.rs` only loads config, expands worktrees, hands to `HiveApp`. Everything else is in the library crate.

- `app.rs` — `HiveApp`: shared `Arc<Mutex<…>>` worker state + view-only fields; `update()` is pure dispatch. Header doc carries the mutation-method naming table (below).
- `workers.rs` — six periodic background threads plus a run-reaper, all the same shape (snapshot under brief lock → work outside lock → write back → `ctx.request_repaint()` → `kick.wait(period)`). Periods, per-tick cost, and rationale are a living table in the module's own header doc (`workers.rs`'s cadence-policy table) rather than duplicated here — re-run `examples/scan_cadence_audit.rs` for fresh real-machine numbers before changing any of them.
- `sync/` — `Kick` (wake signal) and `Status` (one per UI surface so concurrent actions don't clobber).
- `runtime/` — plain-data view types + `expand_worktrees()`.
- `ui/` — the only module that touches egui. `theme.rs` is the single source for semantic colors and glyphs.

## Rust conventions (repo-specific)

- **Explicit re-exports** in each crate's `lib.rs`; no glob re-exports.
- **Examples are debugging tools, not products.** Add `examples/foo.rs` to exercise a `switchbard-core` subsystem against real repos (`probe`, `probe_services`, `classify_check`, `sweep`).
- **HiveApp mutation-method naming** (canonical table in `app.rs` header doc): `open_/cancel_/execute_` (modal lifecycle triad), `add_/remove_/move_` (repo CRUD), `spawn_*` (fire-and-forget threaded mutators, e.g. `spawn_start`, `spawn_kill`).
- **Worktree-first.** One repo can have many worktrees; never collapse them.

## Threading & state ownership

- Worker-visible state lives behind `Arc<Mutex<>>` on `HiveApp`; pure view state (expansion toggles, browser choice) is owned directly by the struct. Filter queries and facets persist per surface through `Config.ui.filters` (`HiveApp::filter`/`filter_mut`) instead of a struct field.
- `Config` is the **single source of truth** for repos + UI defaults; the runtime `repos` Mutex is kept in lock-step by calling `rebuild_worktrees` after every mutation (a genuine DRY invariant — don't add a second store).

## Git invocation safety (named threat — keep it)

- **Never `cd <repo>` in a git invocation** — pass `git -C <path>` instead. The compound triggers a permission prompt in this environment.
- **All git goes through `git_cmd()`** (`git_env.rs`), which scrubs leakable `GIT_*` discovery vars (`GIT_DIR`, `GIT_WORK_TREE`, `GIT_COMMON_DIR`, `GIT_INDEX_FILE`, `GIT_OBJECT_DIRECTORY`, `GIT_NAMESPACE`). Threat: inherited env silently redirecting a command at the wrong repo and corrupting its `.git/config`.
- **`validate_refish`** (`worktree_create.rs`) is the boundary check for untrusted refnames. Downstream trusts it (Rule 5).
