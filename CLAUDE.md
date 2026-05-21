# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What Switchbard is

A macOS-only local dashboard (single native egui window, no webview) that:
- Scans the OS every few seconds for listening processes (`lsof`), attributing each back to a git worktree by walking the process `cwd`.
- Detects what each repo *would* start by reading its own declarations: `Procfile` / `Procfile.dev`, `package.json` scripts, `Makefile`, `docker-compose.yml`, `scripts/*.sh`.
- Probes git state per worktree (dirty, ahead/behind, commit recency).
- Lets the user start a service, stop a process group, kill an external port-squatter, and open `:port` in a chosen browser.

Configuration is persisted at `~/.switchbard/config.toml`. Service logs land in `$TMPDIR/switchbard-logs/`.

## Common commands

```sh
mise install                                     # install pinned Rust from mise.toml
mise run ci                                      # fmt + clippy + test, same as CI/pre-push
mise run hooks:install                           # use .githooks/pre-push in this checkout
mise run bundle                                  # alpha .app bundle in target/release
mise run package                                 # alpha DMG + sha256 in target/dist
mise exec -- cargo build                         # debug
mise exec -- cargo build --release               # ~7 MB optimized
mise run test                                    # full test suite (~0.1s)
cargo test -p switchbard-core <pattern>                # single test by name substring
mise run clippy
mise run fmt
bash scripts/bundle-mac.sh                       # lower-level bundle script
bash scripts/package-dmg.sh                      # lower-level DMG script
```

CI (`.github/workflows/ci.yml`, macos-latest only) installs tools through mise
and runs the `test`, `clippy`, and `fmt` tasks from `mise.toml` on every PR.
The `clippy` and `test` tasks set `RUSTFLAGS=-D warnings`, so any compiler
warning fails the build. The tracked pre-push hook runs `mise run ci`.

### Manual probes (examples)

The `switchbard-gui` crate ships four binary examples for debugging individual subsystems against real repos without launching the GUI:

```sh
cargo run --example probe          -- /path/to/repo [...]   # listener attribution
cargo run --example probe_services -- /path/to/repo [...]   # service detection
cargo run --example classify_check -- /path/to/repo [...]   # SERVER/maybe/not-server verdicts
cargo run --example sweep          -- <command-substring>   # find + kill matching pgids (destructive)
```

## Architecture

Two-crate Cargo workspace; `switchbard-core` has zero UI dependencies and is heavily unit-tested.

### `crates/switchbard-core` — domain layer

Re-exports are explicit in `src/lib.rs`. Mental map of the modules:

- `scanner` — `scan_listeners()`: `lsof`-driven snapshot of `LocalListener` rows.
- `attribution` — joins listeners to `WorktreeRef`s by longest-prefix match on `cwd`.
- `worktree` — `enumerate_worktrees()` shells out to `git worktree list`.
- `workflow` — `detect_services()`: parses Procfile/package.json/Makefile/compose/scripts into `DetectedService` rows.
- `classify` — heuristic verdict (`Server` / `Maybe` / `NotServer`) per detected entry point.
- `expected_port` — best-effort port inference from a command string.
- `resolve` — clusters listeners + detected services into the `ResolvedService` model the UI groups by.
- `git_probe` — `git status --porcelain`, ahead/behind, fetch age, recent commits. All read-only, all spawn `git`.
- `spawn` — `spawn_in_session()`: launches services into their own session/process group so `kill_pgid` can take them down cleanly.
- `kill` — `kill_pgid()` returns a `KillOutcome` (terminated / killed / already gone).
- `open_url` / `BROWSER_APP_NAMES` — `open -a <Browser> :port`.
- `config` — `~/.switchbard/config.toml` load/save; the persisted form is just `Vec<Repo>` + UI defaults.

### `crates/switchbard-gui` — egui/eframe app

`src/main.rs` only loads config, expands worktrees, and hands control to `HiveApp`. Everything else lives in the library crate.

Layout:
- `app.rs` — `HiveApp`: shared `Arc<Mutex<…>>` state for workers, plus view-only fields. `update()` is pure dispatch.
- `workers.rs` — four background threads, all with the same shape (snapshot under brief lock → work outside lock → write back → `ctx.request_repaint()` → `kick.wait(period)`):
  - scanner: 3s
  - git probe: 60s
  - service detection: 30s
  - reaper for spawned runs: 2s
- `sync/` — `Kick` (wake-from-sleep signal) and `Status` (per-surface user-feedback messages — one per UI surface so concurrent actions don't clobber each other).
- `runtime/` — plain-data view types (`ActiveRun`, `WorktreeMeta`, `PickerState`) and `expand_worktrees()`.
- `ui/` — the only module that touches egui. `theme.rs` is the single source for semantic colors and glyphs. `workspace/` is the central panel: per-repo swimlane cards with smart progressive disclosure (worktree rows auto-expand when noteworthy).

### Threading rules

- Worker-visible state lives behind `Arc<Mutex<>>` on `HiveApp`.
- Pure view state (filters, expansion toggles, browser choice) is owned directly by the struct.
- `Config` is the single source of truth for repos + UI defaults; the runtime `repos` Mutex is kept in lock-step by calling `rebuild_worktrees` after every mutation.

## Project-specific conventions

- **macOS-only.** `eframe` is built with `default-features = false` (drops winit's wayland/x11). CI runs `macos-latest` only. Don't add platform-conditional Linux/Windows code unless explicitly porting.
- **No `cd <repo>` in git invocations** — `git` already operates on the current working tree; the compound triggers a permission prompt in this environment. Pass `-C <path>` instead.
- **Examples are debugging tools, not products.** Add a new `examples/foo.rs` when you need to exercise a `switchbard-core` subsystem against real repos.
- **Clippy is gospel in CI.** `RUSTFLAGS=-D warnings` means any new warning fails the build — fix it, don't `#[allow]` it.
- **Worktree-first.** The model assumes one repo can have many worktrees; never collapse them.
