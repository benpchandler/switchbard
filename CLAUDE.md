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

## Gates (firm — CI fails on any)

CI (`.github/workflows/ci.yml`) runs the `fmt`, `clippy`, and `test` mise tasks on both **macos-latest and ubuntu-latest** on every PR. The `clippy` and `test` tasks set `RUSTFLAGS=-D warnings`, so **any compiler warning fails the build — fix it, don't `#[allow]` it.** Run `mise run ci` green before pushing.

## Render-path perf

When touching egui render paths (`crates/switchbard-gui/src/app.rs` or `crates/switchbard-gui/src/ui/**`), run a perf smoke before calling the work done: `SWITCHBARD_PERF=1` with `SWITCHBARD_PERF_LOG=/tmp/switchbard-perf.csv`, exercise Servers scrolling, compare p95 frame/workspace time against the previous build. Avoid full snapshot rebuilds, per-row clones, and unbounded per-frame file lists. Perf-ledger discipline: `docs/perf/README.md`.

## Architecture

Two-crate Cargo workspace. `switchbard-core` has **zero UI dependencies** and is heavily unit-tested; `switchbard-gui` is the only place egui appears.

### `crates/switchbard-core` — domain layer

Re-exports are **explicit in `src/lib.rs`** (no glob re-exports). Module map:

- `scanner` — `scan_listeners()`: per-OS snapshot of `LocalListener` rows (`lsof` / `/proc` behind `cfg`).
- `attribution` — joins listeners to `WorktreeRef`s by longest-prefix match on `cwd`.
- `worktree` — `enumerate_worktrees()` shells out to `git worktree list`.
- `worktree_remove` — dirty-file collection + `remove_worktree()`. The only destructive git op in core.
- `worktree_create` — `validate_refish()` rejects empty / whitespace / leading-dash refish before `git worktree add` (the repo's one true untrusted-input boundary — Rule 5).
- `workflow` — `detect_services()`: parses Procfile/package.json/Makefile/compose/scripts into `DetectedService`.
- `classify` — heuristic `Server` / `Maybe` / `NotServer` verdict per entry point.
- `expected_port`, `resolve` — port inference; clusters listeners + services into `ResolvedService`.
- `git_probe` — read-only `git status` / ahead-behind / fetch age / recent commits.
- `git_env` — `git_cmd()`: every git call goes through it; see Git safety below.
- `spawn` / `kill` — `spawn_in_session()` (own session/process group) + `kill_pgid()` → `KillOutcome`.
- `config` — `~/.switchbard/config.toml` load/save; persisted form is `Vec<Repo>` + UI defaults.

### `crates/switchbard-gui` — egui/eframe app

`src/main.rs` only loads config, expands worktrees, hands to `HiveApp`. Everything else is in the library crate.

- `app.rs` — `HiveApp`: shared `Arc<Mutex<…>>` worker state + view-only fields; `update()` is pure dispatch. Header doc carries the mutation-method naming table (below).
- `workers.rs` — four background threads, all the **same shape** (snapshot under brief lock → work outside lock → write back → `ctx.request_repaint()` → `kick.wait(period)`): scanner 3s, git probe 60s, service detection 30s, run-reaper 2s.
- `sync/` — `Kick` (wake signal) and `Status` (one per UI surface so concurrent actions don't clobber).
- `runtime/` — plain-data view types + `expand_worktrees()`.
- `ui/` — the only module that touches egui. `theme.rs` is the single source for semantic colors and glyphs.

## Rust conventions (repo-specific)

- **Explicit re-exports** in each crate's `lib.rs`; no glob re-exports.
- **Examples are debugging tools, not products.** Add `examples/foo.rs` to exercise a `switchbard-core` subsystem against real repos (`probe`, `probe_services`, `classify_check`, `sweep`).
- **HiveApp mutation-method naming** (canonical table in `app.rs` header doc): `open_/cancel_/execute_` (modal lifecycle triad), `add_/remove_/move_` (repo CRUD), `spawn_*` (fire-and-forget threaded mutators, e.g. `spawn_start`, `spawn_kill`).
- **Worktree-first.** One repo can have many worktrees; never collapse them.

## Threading & state ownership

- Worker-visible state lives behind `Arc<Mutex<>>` on `HiveApp`; pure view state (filters, expansion toggles, browser choice) is owned directly by the struct.
- `Config` is the **single source of truth** for repos + UI defaults; the runtime `repos` Mutex is kept in lock-step by calling `rebuild_worktrees` after every mutation (a genuine DRY invariant — don't add a second store).

## Git invocation safety (named threat — keep it)

- **Never `cd <repo>` in a git invocation** — pass `git -C <path>` instead. The compound triggers a permission prompt in this environment.
- **All git goes through `git_cmd()`** (`git_env.rs`), which scrubs leakable `GIT_*` discovery vars (`GIT_DIR`, `GIT_WORK_TREE`, `GIT_COMMON_DIR`, `GIT_INDEX_FILE`, `GIT_OBJECT_DIRECTORY`, `GIT_NAMESPACE`). Threat: inherited env silently redirecting a command at the wrong repo and corrupting its `.git/config`.
- **`validate_refish`** (`worktree_create.rs`) is the boundary check for untrusted refnames. Downstream trusts it (Rule 5).
