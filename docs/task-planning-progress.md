# Task planning, ordered work and checklist progress

## Authorization and scope

Owner approved implementation and live task migration on 2026-09-15. Latest scope: all repositories tracked by Switchboard, superseding the earlier three-repository selection. Preserve earlier complete-detail and cancellation work. The primary checkout and implementation worktree were clean; continue in the isolated `feat/tui-detail-sections-cancel` worktree after merging current origin/main.

## Model

Planning is an independent field: Considering or Planned. In SBT, `f` then `pl` opens its filter even when the saved view hides the Planning column; `p` chooses Planned and `c` chooses Considering. Explicit outlines such as Project apply within the Planned section as well as remaining tasks. Grouping preserves planned order within each group; outline off restores the complete ordered list without rewriting ranks. Execution status remains independent: Not started, In Progress, Waiting, In Review, Done, Canceled. Planned open tasks have one repository-wide ordered list; retain existing hierarchical/expedite ranking data as compatibility history and use its computed order to seed the new list. New ordinary tasks start Considering; deliberately active tasks may start Planned. Existing saved views remain readable; new Planning and Checklist columns are available and the default view exposes them.

Checklist progress counts each retained acceptance criterion once across a task and its descendant tasks. Show checked/total and percentage; no criteria means unmeasured, not 100%. Exclude canceled/archived branches from open-task work scope. Include own criteria as well as descendants to preserve existing parent contracts. Cycles must terminate and duplicate identities must not inflate counts. Definition of Done is displayed separately and not counted twice. Percent is criterion coverage, not estimated effort.

Done is explicit; checking the last criterion never changes task status. An open task at 100% shows Review. Historical Done with incomplete criteria remains Done and shows its incomplete checklist rather than rewriting history. No mandatory reason for deleting criteria. Git associations remain optional and this slice does not require or generate them.

## Migration and safety

Preview before writes, backup authoritative storage, compare exact expected records at apply, retain identities/content/history, and support idempotent reruns. Map Icebox/Backlog to Considering + Not started; To Do to Planned + Not started; other active states to Planned while preserving execution state. Preserve Done and historical lifecycle. Unknown statuses must be surfaced and preserved, not guessed away. Never change a checkbox or infer task completion from prose/commits. Apply through core-owned commands, never hand-edit live task files or write SQL directly. Migration applies only to configured task repositories; absent task repositories are explicitly skipped. Per-repository transaction boundaries and recovery receipts must be explicit.

## Outcomes and ownership

- Core model, progress and order: domain worker; tests over source, nested, empty, canceled, cycles and explicit/manual completion.
- Preview/apply migration: migration worker; identity/content conservation, stale refusal, backup and replay evidence.
- TUI columns, editing and ordering: TUI worker; real-key/render tests, default/filter/sort/detail and narrow states.
- CLI commands, inventory, integration, docs, live migration and installation: root.
- Final independent review: separate read-only auditor.

## State/stress matrix

| Area | Evidence required |
| --- | --- |
| Planning/status | independent changes, new tasks, invalid planning, unknown legacy states |
| Progress | 2x4 criteria = 8; one check = 12.5%; uneven subtask sizes; own + nested; zero; canceled subtree; cycle; 100% review; Done incomplete |
| Ordered list | add/remove/reorder, no cross-repo identities, legacy order retained, new planned task appended, Done hidden from active order |
| TUI | full and empty records, collapsed sections, keyboard/pointer, saved view compatibility, filter/sort/group, small and wide terminals |
| Writes | stale snapshot, missing record, central and legacy, idempotency, rollback/conservation, ordinary local failure |
| Migration | dry-run totals, backups, per-repo apply receipts, unmapped states, postread parity, unchanged criteria/title/relations/source |

No browser/touch/remote loading states apply to these local terminal controls. Live migration and installation are already authorized; no additional approval checkpoint is inferred.

## Controls and agent commands

In the TUI, `t l` changes Planning, `t r` orders Planned work, and `o l` groups by Planning. Existing saved views keep their columns; new views show ID, Planning, Status, Priority, Checklist and Title. Detail sections expose the available modeled fields and support folding. `t c c` opens cancellation confirmation. Done remains an explicit status action.

Agents use `sb planning list` for JSON planning, order and checklist observations, `sb planning set ID Planned` or `Considering` for planning, and `sb planning rank ID --top|--before ID|--after ID` for order. Planning changes never check criteria or change execution status. Existing `sb list` TSV stays compatible. Use `sb planning preview --out PATH` then `sb planning apply --plan PATH --backup-dir DIR` for migration; preview and receipt files contain private task content and belong outside the repository.

## Rollout

Install the updated CLI, TUI and native app before applying live previews. The old app embeds an older core writer and can create To Do tasks without Planning; ordinary surgical edits preserve unknown Planning fields, but that does not make the old app a compatible creation surface. The previous main build does not understand the migrated planning/status contract; the automatic main installer must not replace the new build with it. Every repository receives an exact preview, backup and atomic apply receipt, followed by conservation and idempotency checks. Historical Done records are preserved even when their criteria remain unchecked.

## Rehearsal evidence (2026-09-15)

Read-only schema-2 previews cover all seven configured repositories with task data: budget (936), matterline (103), MusicProduction (558), switchbard (253), visual-review (8), hub (23), and CambridgeKitchens (86). Five other configured repositories have no task data and are skipped. Total: 1,967 records, 588 legacy execution-status mappings, 600 Planned open tasks, zero unknown statuses. Conservation checks preserve criteria/body bytes, other task fields, timestamps, identities, history, unrelated config and old ranking content. Live migration has not yet run. Private previews and diffs are stored outside the repository under `~/.switchbard/backups/planning-20260915T211622Z/rehearsal-20260915T212639Z`. Fresh previews must be regenerated immediately before apply.

The complete core suite passed 744 tests (2 ignored); the CLI suite passed 72. Independent review passed 7 migration tests and 8 real-key TUI planning tests with no remaining implementation blockers. Formatting and workspace lint passed. Full workspace validation is tracked separately before release. [Terminal buffer evidence](evidence/task-planning-progress/README.md) covers wide, narrow, mixed checklist and detail states. GUI edits are fixture initializers for the added model field, with no GUI render-path behavior change.

## Implementation handoff

Production implementation is present in this change; follow-up commits carry evidence and test-fixture corrections. The complete TUI sweep covered 382 tests (22 existing ignored): 380 initially passed and two fixture assumptions failed, then both corrected suites passed all 17 tests. The corrections align a seeded history entry with its saved four-column view and scroll help before checking an offscreen example; a paint-menu check now verifies its footer without assuming the old column count. No production behavior changed for these corrections. Core, GUI and CLI tests passed during workspace validation; formatting, workspace clippy and developer-gate checks passed. The last unified CI invocation stopped on the old paint expectation; the complete pre-push gate remains the release gate for the final tree.

The native app bundle built from clean commit `6d0473d` and passed strict signing and packaged offline-helper verification. The installed app supplied an independently verified copy of the exact pinned helper payload; its recovered archive passed the normal acquisition verifier with no pin change. The prepared app is under this worktree's mise release target; its private checksum/source receipt is in the rehearsal backup directory. It has not been installed.

Publishing, installation and live migration are outside this documentation phase and remain root-owned release steps. Neither the new binaries nor the live migration have been applied here. The release sequence is to finish the unified gate, publish/merge without losing installed lineage, install updated sb/sbt/native app, refresh the configured-repository inventory and preview every current task repository, then apply through the core command with backups and verify conservation plus idempotency. Do not rely on the recorded preview counts as a future snapshot. All original dirty entries in tracked repositories remain; concurrently created Cambridge diligence files were left untouched.
