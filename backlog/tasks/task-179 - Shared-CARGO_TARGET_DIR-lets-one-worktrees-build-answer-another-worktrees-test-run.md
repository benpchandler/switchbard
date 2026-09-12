---
id: TASK-179
title: Shared CARGO_TARGET_DIR lets one worktree's build answer another worktree's test run
status: Done
assignee: []
created_date: '2026-09-08 12:41'
updated_date: '2026-09-08 20:40'
labels:
  - build
  - ci
  - bug
dependencies: []
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Impact: anyone running `mise run ci` or `cargo test` in a linked worktree. A gate can report a pass or a failure that does not belong to the code in front of it, which is the one thing a gate must never do. Hit today while reproducing TASK-171/173.

Evidence: in a freshly created worktree (`git worktree add` off feat/tui-menu-pickers at 11180ac), `cargo test -p switchbard-tui --test columns` failed three times running with

  left:  "labels added as column 5"
  right: "labels added as column 5 · m then numbers to reorder · esc"

The worktree's own crates/switchbard-tui/src/app/pickers.rs:586 held the long form; a grep for 'added as column' over the whole worktree found no other source for the short one. `strings` on the built test binary under CARGO_TARGET_DIR (~/Library/Caches/switchbard/cargo-target) contained 'added as column' but not 'then numbers to reorder' - the linked switchbard_tui code was not this worktree's. The short form is what the sibling worktree switchbard-tui-menu-pickers had uncommitted at the time, later committed as 6666334. `touch crates/switchbard-tui/src/app/pickers.rs` and rerunning turned the same test green with no source change. Touching every source file in the worktree then gave a fully green `mise run ci`.

mise.toml points CARGO_TARGET_DIR at the platform cache so linked worktrees reuse artifacts (CLAUDE.md, 'Common commands'). That sharing is deliberate and has a real payoff, so the fix is a choice, not an obvious default:

(a) Give each worktree its own target dir - correctness first, pay full rebuilds per worktree.
(b) Keep the shared dir and stop trusting mtime: build with a fingerprint that cannot collide across worktrees, or have the gate touch sources before running.
(c) Keep the shared dir and add a cheap guard the gate runs first - a check that the linked artifact came from this worktree's sources - so a stale answer fails loudly instead of passing.

Recommend (a) for the gate specifically while keeping sharing for interactive builds, but that is the owner's call.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 A test run in a linked worktree provably links that worktree's sources and no other's, demonstrated with the reproduction above (two worktrees differing in one string literal)
- [x] #2 `mise run ci` in a worktree created seconds earlier, with no touch step, agrees with the same command run after touching every source file
- [x] #3 The chosen option is recorded in CLAUDE.md next to the existing CARGO_TARGET_DIR note, so the next agent knows which guarantee it has
<!-- AC:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Chose option (a): one Cargo target directory per worktree, keyed by that worktree's own path (mise.toml, {{ config_root | basename }}-{{ config_root | hash(len=8) }} under the platform cache). Root cause is not mtime alone: Cargo hashes path packages relative to the workspace root so builds stay relocatable, which makes two worktrees of this repo produce byte-identical artifact filenames; mtime is then the only separator, and a fresh worktree's sources are older than a sibling's newer build.

Reproduced first, both directions, with two detached worktrees off 4bf20c2b differing in one string literal in crates/switchbard-tui/src/app/pickers.rs and a shared target dir. Both resolved to the same artifact, columns-4d9bc72ae023007d. The false-FAIL the report captured reproduced, and so did a worse false-PASS: worktree B's columns test passed asserting 'labels added as column 5 · m then numbers to reorder · esc' while no source file anywhere in B contained that string. Under the fix, B fails on its own sources with exactly the reported left/right, and A passes on its own sources with no touch step.

AC #2: a worktree created seconds earlier ran mise run ci green with no touch (exit 0, 77 test-result lines); after touch on every .rs the same command gave the identical result (exit 0, 77).

Also hardened the gate against the same fault arriving by inheritance: .githooks/pre-commit and pre-push now unset CARGO_TARGET_DIR alongside the GIT_* discovery vars, since an exported value overrides mise's per-worktree default. scripts/test-git-hooks.sh proves it and was proven to fail without it (exit 91, 'CARGO_TARGET_DIR leaked into mise'). mise run test-dev-gates green.

The chosen guarantee and the reason two worktrees must never share a target directory are recorded in CLAUDE.md next to the existing note, and in README.md. Added scripts/prune-cargo-targets.sh + mise run target-prune, dry-run by default, which asks mise for each live worktree's directory rather than recomputing the naming rule, so mise.toml stays the sole authority. It reports the old shared layout (62G debug + 1G release) as unowned; not removed, that is shared state and needs the owner's go.
<!-- SECTION:FINAL_SUMMARY:END -->
