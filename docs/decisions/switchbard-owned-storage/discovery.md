# Storage discovery

Read-only audit of revision `4bf20c2b`, 2026-09-08. This inventory identifies current authorities; it does not select the new storage design.

| Records | Current authority | Consumers and evidence |
| --- | --- | --- |
| Tasks, drafts, completed and archived tasks | Repo `backlog/` Markdown | `switchbard-core/src/backlog/parse.rs:20`, `types.rs:150`; shared native mutations in `backlog/mod.rs` |
| Projects and initiatives | Repo `backlog/projects/` and `backlog/initiatives/` | `backlog/hierarchy.rs`; loaded with tasks, computed rollups |
| Goal definitions and dated check-ins | Repo `backlog/goals.yml` | `backlog/goals.rs:1`; CLI goals and TUI derived goal column |
| Manual rank | Repo `backlog/ranking.yml` | `backlog/ranking.rs:1`, applied by `parse.rs:59` |
| Cross-repo ordering | Optional hub `ordering.yml` | `backlog_triage.rs:153` and `:202` |
| Tracked repos and GUI preferences | Machine `~/.switchbard/config.toml` | `config.rs:19`, atomic save at `:332` |
| TUI saved views | Global and repo `.switchbard/views.lua` | `switchbard-tui/src/views.rs:246` and `:269` |
| Live session claims | Machine `~/.switchbard/work/` | `work_sessions.rs:9`, `:27`, atomic replacement at `:313` |
| Mission state | xplan-owned storage | `mission_projection.rs:1`; Switchbard is a reader/supervisor |

Paths above are within `crates/`; unqualified filenames are in `switchbard-core/src/`.

## Consumers that must remain coherent

The GUI, TUI, `sb`, Rust dispatcher, and Python orchestrator must agree on the same records. The Python orchestrator uses `sb queue`; keeping that protocol coherent is necessary even if its code never accesses a database. CLI and TUI startup currently require a physical backlog directory. Current models also expose task source-file paths. A storage change must replace or adapt these assumptions, not only replace file writes.

## Compatibility and migration

Existing compatibility patterns include config versioning and broken-file backups, legacy `milestone:` reads with write-time `project:` migration, legacy serialized view keys, and tolerant goals/ranking loaders. No task-domain central-store import/export or migration mechanism was found in the bounded audit.

Task allocation scans sibling worktrees and active local branches, then reserves IDs in the common Git directory. Migration must inventory divergent and uncommitted records rather than import only the primary checkout. Same-number tasks in different repositories must remain distinct. A repository path move, separate clone, and linked worktree are separate identity cases needing explicit treatment.

## Decisions that affect implementation

1. Which records move: task-domain records or every Switchbard-owned record.
2. Whether independent clones and machines must share or transfer changes.
3. Whether repo summaries serve humans, agents, backup/restore, or more than one purpose.
4. How import resolves divergent versions and how old binaries are prevented from creating a second authority after cutover.
5. How backup and restore preserve writes after cutover; simply reverting code cannot restore newer centralized records to old files.

## Handoff

Next step: resolve the pending owner scope question, then compare storage alternatives and write the complete decision contract with synthetic migration/conflict cases and an independent audit. Do not implement a schema or cut over production storage from this discovery document. TASK-147 remains incomplete.
