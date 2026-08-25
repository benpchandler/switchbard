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

- **Rule 4 (functions/modules short) — one oversized UI file:**
  `crates/switchbard-gui/src/ui/workspace/mod.rs` (~1818 LOC) violates the repo's own
  small-module ethos (standards Rule 6). This is **debt to split**, tracked in
  `docs/product-trajectory.md` → Known gaps. Do not treat its existence as license
  to add more; new UI work should carve toward smaller modules, not pile onto it.

  The former second entry here, `ui/backlog.rs` (~1710 LOC), has since been split
  into `ui/backlog/` — 19 files, largest `board.rs` at ~883 LOC. That is the shape
  the workspace split should aim for.
