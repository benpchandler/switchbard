# Parent task picker objective and evidence

Owner outcome: choose an existing parent in the task picker with its ID and title visible, using shared relationship validation. TASK-182 owns core resolution and eligibility; TASK-183 owns the TUI flow. Both were created and live-claimed in the owner backlog before implementation.

Decision: use the current task-menu branch as the baseline (7ebf1820), preserving the menu the owner is using. Parent movement retains the existing one-level hierarchy and ID reallocation semantics. The picker discloses the ID change. This slice does not change pinned-family presentation or mutate existing owner task relationships.

## State and stress matrix

| State | Verification |
| --- | --- |
| Existing eligible tasks, current parent, title and ID search | Real-key task_parent E2E tests and core relationship tests |
| No eligible parents, no matching search, no selected task | E2E empty/blocked states; explicit message |
| Self, nested parent, task with children, non-active source | Shared core eligibility tests; excluded choices or explicit failure |
| Save, no-op, remove parent, cancel/back, reopen/reload | Disk-backed E2E journeys; verify ID and parent after reload |
| Stale/deleted target or source, mutation failure | Fresh core validation; E2E error and retry, no fabricated success |
| Narrow/short/zero terminal; Unicode, long titles, duplicate titles, many options | Terminal TestBackend layout and keyboard journeys |
| Repeated actions, type-ahead, page navigation | Enter required; stable source ID and existing picker navigation |
| Loading/saving | Local synchronous filesystem operation, same as existing task fields; no network dependency |
| Offline, permissions, partial failure | Local write error reported; no network/reconnect state; existing move has nontransactional multi-file updates |
| Touch/browser zoom/roles | N/A: terminal keyboard surface without roles or browser |

Native terminal appearance review and installed build verification remain pending until recorded below. Tests do not constitute human visual approval.

## Verification before installation

Eight real-key parent journeys passed in `crates/switchbard-tui/tests/task_parent.rs`; the full TUI suite passed 125 tests with 18 existing opt-in tests ignored. Core full suite passed 584 tests with one existing ignored test; after the mixed-prefix corrections all 149 backlog tests passed, including seven parent-focused tests. Core and TUI all-target clippy passed with warnings denied. The installer repeats the required TUI gate on the final source.

Independent core-worker review of the TUI found no blocking source/stale-selection issue. Command review found and corrected both mixed-prefix cases: candidate eligibility now uses repository configuration, and reparenting preserves the chosen task's actual canonical ID when legacy TASK and configured LED prefixes coexist. Empty search remains open for correction; numeric input searches instead of selecting a row.

Remaining limitations: the existing native move operation is not a transaction across task, dependencies, ranking and goals; errors are shown and the task list reloaded. Runtime live-work claims keyed by the old task ID are not renamed by the existing move operation. This feature does not claim to repair those pre-existing contracts. Human native-terminal appearance approval is not recorded. No GUI render path was modified, so GUI frame perf smoke is not applicable.

## Installed delivery

Source commit `b73cb28b` is based on merged main `f5c5f61c` (identical source tree to the earlier menu baseline). `CARGO_TARGET_DIR=/tmp/switchbard-parent-picker-target mise run tui-install` passed the final formatting, clippy and TUI test gates and installed the release executable. The installed executable and isolated release artifact both have SHA256 `7e358fee8d039743403e21bb17e0ccc852cb45be790a3491f3b754c7a186cfc5`. The final TUI run passed 125 tests with 18 existing opt-in tests ignored; eight new parent journeys passed. Final core backlog regression count is 149.

TASK-182 and TASK-183 are In Review in the owner-visible backlog with all acceptance criteria checked and live claims released after installation. TASK-185 (medium) tracks preserving live work claims when native reparenting changes IDs. Feature code is committed locally; no feature PR, merge, or human appearance approval is claimed. The named worktree is retained for review. All agent-owned changes are committed; pre-existing primary-checkout changes remain untouched.
