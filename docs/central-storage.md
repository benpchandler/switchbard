# Central task storage

Switchbard uses one machine-local SQLite database across repositories. Linked worktrees resolve to the same repository identity. Each native record kind has one authority: existing files until explicitly migrated, then the database. Missing or corrupt established databases fail with recovery guidance instead of falling back to stale files.

## Gradual migration

Install a build with central-storage support in every Switchbard consumer before activation. Start with initiative and project definitions, then config, ranking, goals, and tasks. Project rename and task reparent that affect multiple kinds require all affected kinds to share authority; a mixed-authority command refuses before changing anything.

```sh
sb --repo /path/to/repo storage status
sb --repo /path/to/repo storage migrate --kind project
sb --repo /path/to/repo storage migrate --kind project --apply --preview-digest DIGEST
sb --repo /path/to/repo storage migrate --kind task --resolution-file /private/path/task-resolution.json
sb --repo /path/to/repo storage migrate --kind task --resolution-file /private/path/task-resolution.json --apply --preview-digest DIGEST
```

The preview inventories linked worktrees and relevant local branches. Apply rechecks its digest, validates native content, creates a private verified database backup and source manifest, and switches that kind transactionally. Native Switchbard legacy writers and migration share a repository-identity lock, including linked worktrees; the lock is acquired before authority selection and held through the complete write or activation, with authority rechecked after acquisition. Pause direct Markdown editors briefly during apply: arbitrary external writes cannot join this lock, so the retained digest and source checks remain the final guard. Original files stay unchanged. Git-proven stale committed copies can follow the primary version, with every original variant retained in provenance. Dirty, independently changed, deleted, or otherwise ambiguous copies require reconciliation; modification times never decide which version wins. Read ordinary lists after each stage and compare the expected contents. `storage status` also reports changed, missing, or unreadable retained sources without importing them.

Use `--resolution-file` only for a reviewed exception that the automatic evidence rules refuse. The JSON file is strict and bounded. Every resolution lists all source locators being combined, every inventoried physical path and source digest, exactly one `select` decision, and an explicit `reject` decision for every other source. A target may keep one locator, select an absent-primary source, or collapse several lifecycle locators into one record. Apply re-reads the file and every source; the resolution-file digest is part of the preview digest. Unlisted divergence, changed paths or digests, branch-only bytes without an inventoried physical copy, branch deletion, invalid native output, and invented selected content still refuse cutover.

```json
{
  "version": 1,
  "kind": "task",
  "resolutions": [
    {
      "source_locators": [
        "backlog/completed/led-576 - Canonical.md",
        "backlog/archive/tasks/led-576 - Duplicate.md"
      ],
      "target_locator": "backlog/completed/led-576 - Canonical.md",
      "sources": [
        {
          "path": "/absolute/repo/backlog/completed/led-576 - Canonical.md",
          "digest": "SOURCE_SHA256",
          "decision": "select"
        },
        {
          "path": "/absolute/repo/backlog/archive/tasks/led-576 - Duplicate.md",
          "digest": "SOURCE_SHA256",
          "decision": "reject"
        }
      ]
    }
  ]
}
```

For a historical collision between unrelated task records, keep one resolution at its existing ID and give the other a new `target_locator` plus `"repair_task_id": "LED-675"`. This repair changes exactly one plain top-level `id:` scalar in the selected retained bytes, requires the target filename to begin with that ID, and rejects any other projection change. Preview and the private recovery manifest report the selected source digest, repaired target digest, rejected sources, and complete original bytes. The source Markdown files are never rewritten.

Use `SWITCHBARD_DATABASE=/absolute/path/test.sqlite3` to isolate all ordinary commands during rehearsal. The storage command also accepts `--database PATH`; that flag applies only to the storage invocation.

## Optional collaboration file

```sh
sb --repo /path/to/repo storage export
sb --repo /path/to/repo storage import
```

Export writes `.switchbard/tasks.json` only when requested. Commit and PR this one file for collaboration. Import previews changes and conflicts; apply requires the reviewed snapshot digest and database sequence. An independent clone must explicitly bind to the snapshot repository identity. Missing snapshot records never imply deletion. Causal clocks prevent stale snapshots from rolling back newer work; conflicting changes require explicit resolutions. See `storage import --help` for resolution-file and apply options.

The file carries current record content and explicit tombstones. Text is represented as readable exact UTF-8 lines, with binary fallback for unknown non-text content. Unknown kinds, fields, sections, and nested values survive round trips. Custom field changes need no SQL schema migration. Per-kind content versions are separate from SQL and exchange versions; unknown kinds and newer payload versions remain opaque and cannot be edited by an incompatible client. `storage document --help` exposes complete content editing with revision checks.

## Workspace ordering

Machine-local workspace ordering remains outside repository exchange. `storage ordering --migrate-from PATH` previews its source; `--apply --source-digest DIGEST` activates it after backup. `storage ordering` reads central YAML and reports its current sequence. Replacements require `--write-from PATH --expected-sequence N`. Unresolved references remain preserved; resolved task references follow stable identities across reparenting.

## Recovery

```sh
sb --repo /path/to/repo storage backup --file /private/path/backup.sqlite3
sb --repo /path/to/repo storage --database /private/path/restored.sqlite3 restore --from /private/path/backup.sqlite3
```

Restore requires a new destination, preserves the source backup, verifies complete stored state, and assigns a new causal replica identity. Existing databases are never overwritten by restore. Use the restored database through `SWITCHBARD_DATABASE` after verification. A repository moved on disk can be attached to its existing identity with `storage rebind --repo-id UUID`; an unrelated repository cannot take that binding.

## Rollout evidence

[The TASK-147 ledger](decisions/switchbard-owned-storage/migration-ledger.md) distinguishes implementation tests, temporary real-data rehearsals, and actual migration. [Current coverage](decisions/switchbard-owned-storage/coverage-current.md) records remaining acceptance gaps. A successful rehearsal alone does not establish live cutover.
