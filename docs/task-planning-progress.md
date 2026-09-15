# Task planning, ordered work and checklist progress

## Authorization and scope

Owner approved implementation and live task migration on 2026-09-15. Latest scope: all repositories tracked by Switchboard, superseding the earlier three-repository selection. Preserve earlier complete-detail and cancellation work. The primary checkout and implementation worktree were clean; continue in the isolated `feat/tui-detail-sections-cancel` worktree after merging current origin/main.

## Model

Planning is an independent field: Considering or Planned. Execution status remains independent: Not started, In Progress, Waiting, In Review, Done, Canceled. Planned open tasks have one repository-wide ordered list; retain existing hierarchical/expedite ranking data as compatibility history and use its computed order to seed the new list. New ordinary tasks start Considering; deliberately active tasks may start Planned. Existing saved views remain readable; new Planning and Checklist columns are available and the default view exposes them.

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

Install the updated CLI and TUI together before applying live previews. The previous main build does not understand the migrated planning/status contract; the automatic main installer must not replace the new build with it. Every repository receives an exact preview, backup and atomic apply receipt, followed by conservation and idempotency checks. Historical Done records are preserved even when their criteria remain unchecked.
