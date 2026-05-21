# Hive

A local-first dashboard for the dev servers and git worktrees already running
on your Mac. See what's listening, where it came from, what's dirty, and what
needs to be killed.

> **Status:** alpha. macOS only. The author dogfoods it daily; expect rough
> edges around first-run UX and cross-platform support.

## What it does

- **Watches the OS for listening processes.** Scans every few seconds and
  attributes each listener back to a git worktree by walking the process's
  `cwd`.
- **Detects services from your repos' own declarations.** Reads
  `Procfile` / `Procfile.dev`, `package.json` scripts, `Makefile` targets,
  `docker-compose.yml`, and `scripts/*.sh` — surfaces what each one would
  start and what port it would bind.
- **Tracks git state per worktree.** Dirty / clean, ahead/behind from
  upstream, commit activity (Burst / Active / Slow / Idle).
- **One control surface.** Start a service, stop a process group, kill an
  external listener that's holding the port you need, open `:port` in the
  browser of your choice.

## Install

The recommended way is to build from source — no signed binary, no Gatekeeper
warnings, no Apple Developer fee. Requires a Rust toolchain
(`brew install rust` if you don't have one).

```sh
git clone https://github.com/benpchandler/hive
cd hive
cargo build --release
bash scripts/bundle-mac.sh        # produces target/release/Hive.app
open target/release/Hive.app
```

Or, if you just want the binary in your `PATH`:

```sh
cargo install --git https://github.com/benpchandler/hive --bin hive
hive
```

A Homebrew tap is on the roadmap.

## First run

The app starts with no repos configured. Click **➕ Add** in the right
sidebar and pick a folder containing a git repository — Hive enumerates its
worktrees and starts probing. Repeat for every repo you care about.

Configuration lives at `~/.hive/config.toml` (TOML, hand-editable). Logs of
services Hive started land in `$TMPDIR/hive-logs/`.

## How it's built

Two-crate Cargo workspace:

- **`hive-core`** — domain logic. No UI deps. Owns the listener scanner,
  service detectors, git probes, classifier, port-conflict logic, and the
  `ResolvedService` cluster model. Heavily unit-tested.
- **`hive-gui`** — the [egui](https://github.com/emilk/egui) /
  [eframe](https://github.com/emilk/egui/tree/master/crates/eframe) app.
  Single window, no webview, native binary.

Worker threads handle long-running probes (`lsof`, `git status`,
`git log`) so the UI never blocks. The scanner kicks every 3s; the GUI
re-renders only when state changes.

## Building from source

```sh
cargo build              # debug build
cargo test               # 71 tests, ~0.1s
cargo clippy --all-targets -- -D warnings
cargo fmt --all -- --check
cargo build --release    # ~7 MB optimized binary
```

CI runs the same three checks on every PR.

## Contributing

PRs welcome. Keep changes scoped, run the local checks before pushing, and
include a one-line "why" in the commit body. The codebase favors small
modules and explicit names — read the current source for ground truth.

## License

MIT. See [LICENSE](LICENSE).
