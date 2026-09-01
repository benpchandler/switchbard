# Power-of-10 Overrides — switchbard

Repo-specific application of the global Power-of-10 discipline
(`~/.claude/shared/code-standards.md` → §Power of 10). Canonical templates are the
firm floor; this file records only what is *repo-specific*.

## Threat tier

**LOW** — local-first desktop app: no network service, no telemetry, no account, no
multi-tenant data, no irreversible money/PII operations. The one destructive action
(`git worktree remove`) is behind an enumerated confirmation dialog.

But this is a **public, open-source repo**, so the *legibility / craftsmanship* bar is
HIGH: the code is the project's public face. Treat naming, module size, doc-comments,
and zero-warning builds as load-bearing, not cosmetic. A LOW threat tier does **not**
relax the Power-of-10 floor here; the public-craft bar can only make it stricter.

## Language templates (by link)

| Language | Role | Template |
|---|---|---|
| Rust   | primary (~96%) | `~/.claude/standards/power-of-10/rust.md` |
| Python | scripts only (`scripts/perf-summary.py`, icon tooling) | `~/.claude/standards/power-of-10/python.md` |
| Bash   | scripts only (bundle / package / verify / rebuild-reload) | `~/.claude/standards/power-of-10/bash.md` |

## Earned exceptions

**None.** The repo currently claims no exception to any Power-of-10 rule.

## Known debt (NOT exceptions — pay down, don't grandfather)

- **Rule 4 (functions/modules short) — oversized UI file, discharged (TASK-100):**
  `crates/switchbard-gui/src/ui/workspace/mod.rs` (~1818 LOC) is gone. TASK-100 replaced
  it with `ui/places/ops/` (merged Servers+Workspace into one row-per-worktree table);
  the review that closed the PR caught this split adding a second oversized file
  (`row.rs`, 898 LOC vs. the ~883 `board.rs` target below) and a medic pass carved it
  further, the same way `ui/backlog.rs` did.

  Current shape, `crates/switchbard-gui/src/ui/places/ops/` (9 files):
  `ops.rs` 739 (place entry point + snapshot + modal wiring — not part of the original
  debt, unchanged in scope by this split), `row.rs` 504 (row assembly: which cells, what
  order, capped how many chips), `chips.rs` 480 (Services/Listening per-item chip
  rendering + the tiered Open-button port resolution, carved out of `row.rs`),
  `staleness.rs` 452, `landing.rs` 412, `tooltips.rs` 330 (gained the Git chip's
  staleness/size hover text this pass folded out of `staleness.rs`), `git_chip.rs` 314
  (new: the Git column's single compact chip, TASK-100 medic pass), `bulk_remove.rs` 183,
  `create_worktree.rs` 128, `agent.rs` 72, `rename_worktree.rs` 68. Largest submodule
  (`row.rs`, 504) is now well under the `board.rs` (~883) target that motivated this
  entry — the split holds.

  The former second entry here, `ui/backlog.rs` (~1710 LOC), was split earlier into
  `ui/backlog/` — 19 files, largest `board.rs` at ~883 LOC — the shape both the
  workspace and this Ops split aimed for.
