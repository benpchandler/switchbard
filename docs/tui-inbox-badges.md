# Inbox destination and navigation badges

## Outcome and boundaries

TASK-193 is the owner's high-priority first slice toward visible handoffs. The first row contains Tasks, Pull Requests, and Inbox. Pull Requests counts all open PRs in the repository, independently of bounded history, filters, checks, reviewer, and active page. Positive counts use the shared `attention_badge` theme surface; known zero hides the badge. Inbox is intentionally unpopulated and has no fabricated action count. The collapsible bottom pane, action collection, handoff enforcement, and recent completion history are later work.

The seven abstraction tasks remain a separate implemented branch awaiting PR delivery. This slice does not imply their acceptance or supersede that outcome. TASK-193 must remain In Review with a concrete PR after delivery, pending owner acceptance.

## State and stress contract

| State or stress | Expected behavior | Verification |
| --- | --- | --- |
| First load on Tasks or resumed Inbox | Asynchronous PR observation starts without visiting PR page; unknown is not zero | E2E and actual TUI |
| Known zero / positive count | Zero hidden; positive filled badge on every page | Live GitHub and rendered E2E |
| Bounded or filtered PR history | Count comes from repository total, not loaded or visible rows | Core query contract and live filtering |
| Loading / failure / recovery | Unknown indicator; retained count explicitly stale after failed refresh; never false zero | Rendered E2E |
| Blank Inbox / keyboard mutations | Honest placeholder; no hidden task/PR writes; common navigation/reporting works | Key-driven E2E |
| Navigation / restart / config reload | Page and independent list state survive; theme hot reload updates badges | Key-driven E2E |
| Narrow / standard / wide / short viewport | Destination headings remain reachable, count distinct, hints yield space | Terminal buffer checks |
| Large counts / maximum numeric input | Bounded formatting and no history-window undercount | Parser bounds; narrow rendering gate |
| Permission loss / offline | Read failure uses unknown/stale state and existing recovery notification | Same read-error path; live access revocation not exercised |
| Saving / conflict / duplicate submissions | No Inbox mutation exists in this slice | N/A to blank page; existing list behavior preserved |
| Touch / browser zoom / editable content / multilingual Inbox items | Native terminal, no content collection or editing yet | N/A to this slice; terminal resizing and keyboard apply |

## Evidence

Implementation and local validation complete; PR delivery and owner acceptance pending. Logs: `/tmp/switchbard-inbox-evidence/`. Initial primary checkout dirt preserved. Worktree: `/Users/bpc/Dev/.worktrees/switchbard-inbox`, branch `feat/tui-inbox-badges`.

- `mise run tui-install` passed formatting, all-target TUI clippy, the TUI E2E suite, and release installation. Review binary: `/Users/bpc/.local/share/switchbard-builds/inbox/bin/sbt`. The existing default binary is preserved because it includes unrelated unmerged parent-picker work.
- Exact-total parser and query tests: 18 core PR-list tests passed; core all-target clippy passed. The parser covers zero and counts above the historical row cap.
- Live rendered journeys passed against Switchbard (one open PR) and budget (zero open PRs), covering all three pages, filtering to no matches, theme hot reload, and a real invalid-repo refresh retaining stale data. Terminal sizes: 40x8, 80x24, 120x40, 180x50. Captures are named `sbt-navigation-badges-{0,1}.txt` in the OS temporary directory.
- Inbox tests cover blocked list actions, reporting, legacy resume records, and independent list filters across restart. An actual PTY session observed the startup count on Tasks and opened Inbox via two Tabs.
- Independent review found a legacy-theme compatibility bug. The live E2E first failed with missing badge background, then passed after omitted `attention_badge` inherited `chip`. Explicit overrides remain authoritative. No remaining review blockers.
- Remaining gaps: owner visual acceptance, live large-count repository, actual remote access revocation, and CI on the delivered revision. No GUI render paths changed; egui performance smoke is N/A.
