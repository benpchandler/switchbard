# AGENTS.md

`CLAUDE.md` is the source of truth for this repo. This file is a thin adapter for
non-Claude agent runtimes (e.g. Codex) — not a second policy. Do not duplicate
guidance here; point to the canonical doc instead.

## Precedence

1. System / developer / direct user instructions
2. This `AGENTS.md`
3. `CLAUDE.md` (repo architecture, Rust conventions, threading, git safety, gates)
4. Engineering standards: `~/.claude/shared/code-standards.md`; Rust Power-of-10:
   `~/.claude/standards/power-of-10/rust.md` + repo `power-of-10-overrides.md`
5. `docs/product-trajectory.md` — read before non-trivial work (standards Rule 1)

If runtime guidance appears to conflict with `CLAUDE.md`, follow `CLAUDE.md` and
realign this file.

## Runtime-specific notes (not in CLAUDE.md)

- **Live watch/reload loop.** `.claude/settings.local.json` registers a `Stop` hook
  that runs `scripts/rebuild-and-reload.sh` (rebuild DMG + reload the running app on
  source change). Coordinate around it — heads-up before killing/restarting the app
  or its build, and don't fight the reload loop.
- **`git -C`, never `cd <repo>`.** The `cd <repo> && git …` compound triggers a
  permission prompt in agent runtimes. (Full rationale: CLAUDE.md → Git safety.)
- **Backlog.md task tracker.** File follow-ups as `task-N` markdown under
  `backlog/tasks/` via the `backlog` CLI; never hand-edit the task files.
