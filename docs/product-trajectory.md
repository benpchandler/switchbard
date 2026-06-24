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

## Speculative (do NOT pre-build)

- Windows support. (No `cfg` branches, no CI, no demand recorded.)
- Signed/notarized macOS distribution.
- Any daemon, account, sync, or telemetry — explicitly against the local-first stance.
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
