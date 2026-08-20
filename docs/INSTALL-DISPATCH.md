# Installing `switchbard-dispatch` (headless, optional)

**This is not enabled by anything in the repo.** `switchbard-dispatch` is a
separate, opt-in binary; installing the launchd agent below is a manual step
you take on your own machine, on purpose. Nothing here runs `launchctl load`
or `bootstrap` for you.

## What it does

`switchbard-dispatch` is the GUI-less half of Switchbard's dispatch pipeline
(`docs/product-trajectory.md`, unified task hub slice 3). One run:

1. Reads `~/.switchbard/config.toml` for your tracked repos (the same file
   the GUI writes).
2. For each tracked repo that's a Backlog project, loads its tasks and drains
   up to a small cap of tasks labeled `dispatch` — creating an isolated
   worktree per task, running a headless `claude -p` against it, and opening
   a pull request on success. See the `switchbard_core::dispatch` module doc
   for the full pipeline.
3. Exits. It does not stay resident — see "Why `StartInterval`, not
   `KeepAlive`" below.

Nothing auto-flags a task for dispatch. You (or the GUI, once it grows the
Dispatch button) label a task `dispatch` via the `backlog` CLI; this binary
only drains whatever's already labeled that way.

## Requirements

- `git`, `gh` (authenticated: `gh auth status`), `claude`, and `backlog` on
  `$PATH`.
- Repos in `~/.switchbard/config.toml` that use Backlog.md and have a GitHub
  remote you're allowed to open PRs on.

## Build it

```sh
mise run ci                                    # same gate as CI
cargo build --release -p switchbard-dispatch
./target/release/switchbard-dispatch           # one drain pass, prints a summary
```

## Configuration

Environment variables only — no CLI flags, no second config file:

| Variable | Default | Meaning |
|---|---|---|
| `SWITCHBARD_DISPATCH_BASE_BRANCH` | `main` | Branch new dispatch worktrees are created from. |
| `SWITCHBARD_DISPATCH_CLAUDE_BIN` | `claude` | `claude` binary to invoke (resolved via `$PATH` if bare). |
| `SWITCHBARD_DISPATCH_REMOTE` | `origin` | Git remote the dispatch branch is pushed to. |
| `SWITCHBARD_DISPATCH_MAX_CONCURRENT` | `2` | Cap on queued tasks drained per run. |
| `SWITCHBARD_DISPATCH_STALE_AFTER_SECS` | `1800` | Advisory only (TASK-46) — no longer a kill trigger. This binary itself does nothing with it beyond passing it through `DispatchOptions`; it only matters if a GUI reading the same tracked repos uses it to decide when a run's chip/Dispatch-view row flips to needs-attention. |

A repo using a default branch other than `main` needs
`SWITCHBARD_DISPATCH_BASE_BRANCH` set per invocation today — this binary has
no per-repo override yet.

## Running it periodically (launchd)

A template agent lives at
[`assets/launchd/com.switchbard.dispatch.plist`](../assets/launchd/com.switchbard.dispatch.plist).
It uses `StartInterval` (every 15 minutes by default), not `RunAtLoad` +
`KeepAlive` — each interval spawns the binary fresh, it drains, it exits.
There is no long-lived `switchbard-dispatch` process at any point. That's the
`docs/product-trajectory.md` "owner-scoped exception" to the local-first
"no daemon" stance: a scheduled batch job, not a resident daemon, account, or
sync service.

To use it:

1. Build the binary (above) and note its absolute path.
2. Copy the template: `cp assets/launchd/com.switchbard.dispatch.plist ~/Library/LaunchAgents/`
3. Edit the copy: replace `ProgramArguments`' placeholder path with your
   binary's real path, and fix the `PATH` entry under
   `EnvironmentVariables` so it covers wherever `git`/`gh`/`claude` actually
   live on your machine.
4. Only once you're ready for it to actually run on a schedule:
   ```sh
   launchctl bootstrap gui/$(id -u) ~/Library/LaunchAgents/com.switchbard.dispatch.plist
   ```
   To stop it later: `launchctl bootout gui/$(id -u)/com.switchbard.dispatch`.

Logs land at `/tmp/switchbard-dispatch.log` / `.err.log` (the plist's
`StandardOutPath`/`StandardErrorPath`) plus the per-task logs under
`$TMPDIR/switchbard-logs/` that `dispatch_one` already writes.

## Current limits

- One base branch per invocation (see above) — no per-repo override.
- No backoff/retry policy beyond `drain_dispatch_queue`'s single gh-403
  circuit breaker per run; a persistently rate-limited token needs a human to
  notice the `.err.log`.
- Alpha software, same as the rest of Switchbard — read the pipeline's module
  doc (`crates/switchbard-core/src/dispatch.rs`) before pointing it at a repo
  you care about.
