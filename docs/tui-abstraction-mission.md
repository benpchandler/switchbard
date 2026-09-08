# Abstraction implementation mission

Objective: prioritize and implement TASK-144 and TASK-162 through TASK-167, exercise the actual sbt TUI throughout, and document encountered defects and feature requests through the native tracker.

## Ordered outcomes

1. TASK-162: shared column capabilities, entity adapters, deterministic sorting.
2. TASK-163: shared field parsing and matching through those adapters.
3. TASK-164: semantic paint precedence separated from terminal colors.
4. TASK-167: authoritative filing and merge dates in existing paint controls.
5. TASK-165: bounded presentation contracts for Tasks and Pull Requests.
6. TASK-166: explicit feature-scoped view capabilities and compatible persistence.
7. TASK-144: one keyboard action catalog for configuration, availability and help.

All retain medium priority from their recorded user/developer impact. Ordering follows dependencies; independent presentation and shortcut work may overlap. Actual primary tracker rank was updated with sb. Implementation starts at origin/main f5c5f61c, which includes the existing PR parity and picker consolidation; the user checkout remains on its original branch with its existing changes preserved.

## Acceptance and authority

Each issue's acceptance criteria remain authoritative. No human confirmation is inferred for TASK-144. Root owns integration, tracker closeout, verification, installation coordination and final claims. Teammates own disjoint source leases. No GitHub merge, PR write, or remote publication is required by this request. Runtime sbt interaction and the documented local install loop are in scope; shared GUI restart is excluded.

## State and stress evidence plan

| Dimension | Required evidence |
| --- | --- |
| Tasks/PR default and active | Real key events, disk-backed tasks, rendered cells and native PTY session |
| Empty and zero matches | Filter and list E2E journeys |
| Loading, stale, failure, retry | Existing PR observation journeys plus list contract coverage |
| Dirty drafts and save/restart | Existing task draft tests and slot/restart E2E journeys |
| Missing/invalid field and date | Matching and date paint E2E journeys |
| Long labels, multi-value, duplicates | Column, filter and picker rendered assertions |
| Narrow/current/wide and short viewport | 42/80/140 column terminal buffers, bounded picker/list navigation |
| Keyboard focus, repeated actions, page switches | Real-key journeys, independent Tasks/PR state |
| Large lists and bounded rendering | Offscreen/scroll/selection tests at documented N |
| Pointer/touch/browser zoom | N/A: terminal keyboard surface; terminal dimensions cover resizing |
| Remote mutation permissions | N/A: abstraction and read-only PR paint do not add remote actions |
| GUI render performance | N/A unless GUI render paths change; existing GUI primitives are inventoried read-only |

Evidence and remaining gaps are updated here and in the task-specific evidence documents. A green test or implemented slice does not close the seven-task objective.

## Initial observations

The installed sbt launched successfully in an agent-owned PTY at 80x24 and rendered Abstraction in the requested rank order with TASK-162's live claim. Existing primary changes were recorded outside the repository under /tmp/switchbard-abstractions-evidence. Shared target provenance risk is already tracked as TASK-179; this mission uses an isolated Cargo target for authoritative verification.

## Verified closeout

Implementation code is committed as `815bcf01` on `feat/tui-abstractions`. The six tasks TASK-162, 163, 164, 167, 165 and 166 are Done in the canonical primary-checkout tracker. TASK-144's engineering criteria pass and its code is implemented; it remains In Review solely because its original AC1 requires the human reporter's confirmation. All mission claims were released through `sb work release`; that remaining criterion was explicitly recorded, never checked on the reporter's behalf.

The tracker rank remains 162, 163, 164, 167, 165, 166, 144, all medium priority based on their recorded impacts. Tracker edits remain in the user's existing checkout alongside its pre-existing dirty state, rather than sweeping unrelated work into a commit. Seven unrelated pre-existing tracked diffs were compared against the captured baseline and remained byte-identical; concurrent unrelated additions were preserved. The implementation worktree is independently committed.

### Final verification

- `CARGO_TARGET_DIR=/tmp/switchbard-abstractions-target cargo test -p switchbard-core`: 582 passed, one existing opt-in test ignored; all-target core clippy passed.
- `CARGO_TARGET_DIR=/tmp/switchbard-abstractions-target CARGO_INSTALL_ROOT=/Users/bpc/.local/share/switchbard-builds/abstractions mise run tui-install`: exit 0; formatting and all-target TUI clippy passed; 141 TUI tests passed, 19 opt-in tests ignored in the ordinary gate; optimized binary installed.
- Seven relevant opt-in journeys separately passed against authenticated read-only GitHub and the real work store: six PR-control journeys and one authoritative merge-date paint/restart journey. The final live-claim invocation initially omitted its required `SBT_WORK_TASK`; rerunning that check with `SBT_WORK_TASK=TASK-162` passed. Other live journeys were green. No product fix or skipped assertion was used to resolve that invocation error.
- Real PTY dogfooding used the original installed sbt for ranked-task inspection, live claims, help, and all five bug filings. The newly installed isolated release binary then exercised 80x24 scrolling help (complete report instructions), task filing-date paint, page switching, real 100-row PR history and merge-date painting. Both agent-owned terminal sessions exited normally.
- The isolated binary is `/Users/bpc/.local/share/switchbard-builds/abstractions/bin/sbt`, SHA-256 `12ae567900fb6ecc5416595fe4b469f24e3e06165788e3db9e6126970532614a`. The user's default sbt was preserved because it contains the unmerged parent-picker feature from another branch. No shared GUI process was restarted.
- No GitHub push, pull request, merge, deployment or Linux CI run is claimed. GUI rendering was not changed, so its render-path performance gate is not applicable. Tests, buffers and PTY interaction do not constitute human visual approval.

### Issues filed through sbt and fixed

| Issue | Priority and user impact | Verified repair |
| --- | --- | --- |
| TASK-188 | Medium: terminal users could not read full help, and help navigation moved hidden selection | Wrapped scrolling help, configured keys, selection preserved; 80x24 and 40x8 regression plus real PTY |
| TASK-189 | Medium: users leaving relative-date views open saw stale membership across UTC midnight | Both cached page projections invalidate at the next UTC-day-change tick, including offline PR cache |
| TASK-190 | Medium: users with one list body row could not see the selected grouped task | Keep heading only when at least two body slots fit; real-key resize/navigation regression |
| TASK-191 | High: users with sparse view slots could lose saved definitions or load a different numbered slot | Reject sparse global sequences without overwrite; retain explicit sparse repo slot identities |
| TASK-192 | Medium: users encountering a partial promotion could not retry after repair | Report confirmed global write, retain repo override, update source guard and allow safe retry |

All five issues are Done with checkable criteria and evidence recorded through sb. Additional test maintenance corrected a PR-filter helper that appended instead of replacing the query and updated an old no-match probe now that the new filed column legitimately matches `f`. Legacy `cw` remains available by using canonical short names in column controls and descriptive date labels only in painting controls.

### Remaining boundary

No implementation or automated verification gap remains for the seven requested engineering outcomes. Human reporter confirmation for TASK-144 remains open. The default installation and remote delivery are intentionally separate from this verified isolated build. Date projections refresh at the next app tick after midnight; no strict across-every-operation single-frame clock snapshot is claimed.
