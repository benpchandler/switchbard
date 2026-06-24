<h1 align="center">Switchbard</h1>

<p align="center"><em>One window for every agent, worktree, and port on your machine.</em></p>

<p align="center">
  <a href="https://github.com/benpchandler/switchbard/releases"><img src="https://img.shields.io/github/v/release/benpchandler/switchbard?include_prereleases&sort=semver&color=2b8a3e" alt="Latest release"></a>
  <a href="https://github.com/benpchandler/switchbard/actions/workflows/ci.yml"><img src="https://img.shields.io/github/actions/workflow/status/benpchandler/switchbard/ci.yml?branch=main&label=CI" alt="CI status"></a>
  <a href="https://github.com/benpchandler/switchbard/releases"><img src="https://img.shields.io/github/downloads/benpchandler/switchbard/total?color=555&label=downloads" alt="Downloads"></a>
  <img src="https://img.shields.io/badge/platform-macOS%20%7C%20Linux-lightgrey" alt="Platform: macOS and Linux">
  <a href="LICENSE"><img src="https://img.shields.io/github/license/benpchandler/switchbard?color=blue" alt="License: MIT"></a>
</p>

<p align="center">
  <img src="docs/assets/switchbard-agent-worktrees.png" width="860" alt="Switchbard showing several agent worktrees and the local services each one is running">
</p>

When Claude, Codex, and other agents each hack in their own git worktree, your
machine quietly fills up with local servers, dirty branches, and mystery ports —
and `localhost:3000`, `:5173`, and `:8080` all start to blur together.

**Switchbard is a native macOS/Linux dashboard that shows what's listening,
which worktree it came from, and whether it's safe to open, stop, or clean up —
all in one window, with no telemetry and no cloud account.**

> **Status: alpha.** macOS ships a downloadable DMG; Linux builds from source.
> The author dogfoods it daily — expect rough edges around first-run UX and
> packaging.

## Features

- 🔍 **Sees every listener, live.** Scans the OS every few seconds for processes
  bound to a port — no guessing which terminal tab owns `:3000`.
- 🧭 **Attributes processes to worktrees.** Walks each process's `cwd` to map a
  listener back to the exact repo and git worktree that started it.
- 🧩 **Detects services from your repo's own files.** Reads `Procfile` /
  `Procfile.dev`, `package.json` scripts, `Makefile` targets,
  `docker-compose.yml`, and `scripts/*.sh`, and surfaces what each would start
  and which port it would bind.
- 🌱 **Full worktree lifecycle.** Create a worktree (new or existing branch),
  give it a memorable label, and remove it when you're done — the remove dialog
  enumerates uncommitted changes and running services, and can optionally delete
  the local branch with per-check safety (merged into main? checked out
  elsewhere?) so you never drop unlanded work by accident.
- 📊 **Git state at a glance.** Dirty / clean, ahead / behind upstream, and
  commit activity (Burst / Active / Slow / Idle) per worktree.
- 🎛️ **One control surface.** Start a service, stop a process group, kill an
  external squatter holding the port you need, or open `:port` in the browser of
  your choice.
- 🔒 **Local-first.** No telemetry, no account, no background daemon. Config is a
  hand-editable TOML at `~/.switchbard/config.toml`.

## Install

### macOS

Download the latest `Switchbard-*-macos-arm64.dmg` from the
[**Releases page**](https://github.com/benpchandler/switchbard/releases), open
it, and drag `Switchbard.app` into `Applications`.

> Switchbard is unsigned and unnotarized. On first launch, open it from Finder
> with **Control-click → Open**, then confirm the unidentified-developer prompt.
> Full notes: [docs/INSTALL-MAC.md](docs/INSTALL-MAC.md).

<details>
<summary>Build from source instead</summary>

```sh
git clone https://github.com/benpchandler/switchbard
cd switchbard
cargo build --release -p switchbard-gui
bash scripts/bundle-mac.sh        # produces target/release/Switchbard.app
open target/release/Switchbard.app
```

Or put the bare binary on your `PATH`:

```sh
cargo install --git https://github.com/benpchandler/switchbard --bin switchbard
```
</details>

### Linux

Download the prebuilt `switchbard-*-linux-x86_64.tar.gz` from the
[**Releases page**](https://github.com/benpchandler/switchbard/releases), unpack
it, and run the binary:

```sh
tar -xzf switchbard-*-linux-x86_64.tar.gz
./switchbard-*-linux-x86_64/switchbard
```

The binary `dlopen`s a few shared libraries at runtime (libxkbcommon,
libwayland / X11, libGL) and uses `xdg-open` to launch ports — any normal
desktop session already has them. No `.deb` / `.rpm` / `.AppImage` (or ARM
build) yet; see [docs/INSTALL-LINUX.md](docs/INSTALL-LINUX.md) for those and for
building from source. Switchbard reads Linux listeners straight from `/proc`, so
it never needs `lsof`.

## Quick start

1. Launch Switchbard — it opens with no repos configured.
2. Click **➕ Add** in the right sidebar and pick a folder containing a git
   repository. Switchbard enumerates its worktrees and starts probing.
3. Repeat for every repo you care about. Rows light up as services start,
   branches drift, or ports get held.

Configuration lives at `~/.switchbard/config.toml`. Logs of services Switchbard
started land in `$TMPDIR/switchbard-logs/`.

## How it works

Switchbard is a two-crate Cargo workspace with no webview — a single native
[egui](https://github.com/emilk/egui) /
[eframe](https://github.com/emilk/egui/tree/master/crates/eframe) window:

- **`switchbard-core`** — domain logic, zero UI deps: the listener scanner,
  service detectors, git probes, port-conflict classifier, and the
  `ResolvedService` model. Heavily unit-tested.
- **`switchbard-gui`** — the egui app. Worker threads run the long probes
  (`lsof` on macOS, `/proc` on Linux, `git status`, `git log`) off the UI
  thread, so the window never blocks. The scanner kicks every 3s and the GUI
  re-renders only when state changes.

## Development

```sh
mise install        # pins Rust 1.95.0 from mise.toml (mise is optional)
mise run ci         # fmt + clippy (-D warnings) + the full test suite
mise run bundle     # macOS: Switchbard.app in target/release
mise run package    # macOS: DMG + sha256 in target/dist
```

Prefer plain Cargo? Every task above maps to the obvious `cargo fmt` /
`cargo clippy` / `cargo test` / `cargo build --release` invocation. CI
(`.github/workflows/ci.yml`) runs the mise tasks on macOS and Linux on every PR.
Run `mise run ci` (or the equivalent Cargo commands) green before pushing.

## Contributing

PRs welcome. Keep changes scoped, run `mise run ci` before pushing, and include a
one-line "why" in the commit body. The codebase favors small modules and explicit
names — read the current source for ground truth.

## License

MIT — see [LICENSE](LICENSE).
