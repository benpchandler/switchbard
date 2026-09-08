# TUI task creation

Objective: capture an ordinary task in the current repository without leaving sbt. TASK-170 tracks implementation. The existing core write layer remains the authority for validation, defaults, allocation and persistence. Scope is title capture; task metadata continues to use existing editing tools.

Initial state: linked worktree feat/tui-add-task was clean at 3ed538a. Primary checkout had unrelated modifications to backlog/ranking.yml and TASK-150; neither is included in this work.

## State and stress matrix

| State | Required evidence |
| --- | --- |
| Default, empty repository, populated list | Real-key E2E creates and selects a persisted task |
| Active draft, blank, whitespace, editing, cancel | E2E: no premature writes; visible validation; Esc preserves existing tasks |
| Failure, retry, permission/dependency unavailable | Real filesystem obstruction; draft retained; recover and submit once |
| Success, repeated Enter, duplicate titles | E2E checks disk count and distinct IDs |
| Filtered, hidden by standing settings, stale external additions | E2E keeps filter, reports hidden result; allocator avoids collisions |
| Long, unbroken, Unicode content | E2E verifies bound and visible input tail |
| Narrow/current/wide and short viewport | Actual ratatui container at 40x8, 100x20, 160x30; zero-size does not panic |
| Keyboard focus, shortcut remapping, help | Real-key E2E |
| Reload while dirty | Self-update defers while title editor is active; tick preserves draft |
| Loading/saving, outcome unknown, partial failure | Synchronous local core operation; no asynchronous request or remote reconciliation added |
| Historical/read-only roles, access removal, offline/reconnect | No role or network model; local write errors use failure/retry above |
| Pointer/touch/browser zoom | N/A: keyboard terminal UI; resize represents terminal text scaling |
| Many/mixed items, overlaps, multi-line | Existing bounded table projection unchanged; title is a bounded single-line field |

## Evidence

The ten tests in `crates/switchbard-tui/tests/create.rs` pass through the real App key handler, ratatui renderer and filesystem. They cover every applicable matrix row except the binary-replacement trigger itself, which is code-reviewed: main defers re-exec until Browse. The fixture renders use the existing TUI theme at 40x8, 100x20 and 160x30, plus zero-size resilience. No human visual approval claimed. Abrupt process termination does not persist an unsubmitted title draft. Input clipping preserves complete graphemes and accounts for wide characters. Long filesystem error details may clip at narrow widths; the failure prefix and retry controls remain visible.

Fact-source audit: `App::create_task` delegates to `switchbard_core::create_backlog_task`, the same facade used by the CLI. Defaults and persistence remain in core. `finish_new_task` uses the existing filtered `visible` projection to decide whether to select or report hidden; it introduces no second filter predicate. Independent review identified a held-Enter transition that opened detail after creation; browse now ignores Enter repeat events and a real Press-then-Repeat E2E guards it. Final `mise run tui-install` passed: formatting, warning-free clippy, all 86 TUI E2E tests, and installation to `/Users/bpc/.cargo/bin/sbt`. Independent review confirmed the repeat-key fix and reported no remaining findings. Full workspace CI was not run because this is the scoped TUI install loop, not a merge.


## PR-enabled installation correction

The initial install replaced the PR-enabled binary from `feat/tui-pr-merge` with a task-only checkout, hiding the PR tab. The install provenance in Cargo and the previous install output confirmed the regression. The corrected integration starts at `1d14a58` (the installed PR feature revision) and reapplies capture; `a` adds a task while existing `n` notification dismissal remains intact. The new combined E2E checks both successful task creation and switching to Pull Requests and back with selection preserved. The PR page, notification, merge, and refresh suites remain required alongside capture tests. Initial correction worktree was clean at `1d14a58`; unrelated primary-checkout work remains untouched.

Independent review found no remaining functional regression; capture 11/11 and page navigation 4/4 passed. The first shared-target gate passed integration tests but failed rustdoc because another worktree replaced the core rlib: its dependency manifest lacked PR modules despite current source exporting them. Final verification and install use an isolated CARGO_TARGET_DIR at /Users/bpc/Library/Caches/switchbard/cargo-target-tui-pr-restored. No shared build processes were interrupted. No remote PR mutations are authorized or used in verification. Final isolated `mise run tui-install` passed formatting, warning-free clippy, 102 tests and rustdoc; 17 existing opt-in tests were skipped. The installed Cargo provenance now points to this PR-enabled integration worktree. The corrected shortcut is `a`; Tab switches between Tasks and Pull Requests.
