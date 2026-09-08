# Decision: one database, optional repository exchange

Status: the owner confirmed the central-database and optional single-file direction. This detailed implementation proposal still needs review. No production migration has occurred.

## Owner outcome

Switchbard owns and updates one central database for all repositories. Every local worktree reads the same records without task-file commits, merges, or PRs. A repo may optionally carry one file that collaborators commit and PR to exchange planning data between independent databases.

## Proposed model

Use one machine-local SQLite database at ~/.switchbard/switchbard.sqlite3, with an explicit test/alternate-root override. Move native task-domain data there: all task lifecycle states, project and initiative definitions, goals and check-ins, rank, relationships, and task configuration. GUI, TUI, CLI, dispatch, and refine use the same core command layer and database transactions. No running GUI, daemon, cloud service, or network connection is required.

Machine-specific preferences, executable TUI views, process claims, caches, service logs, and Git observations retain their existing stores and do not enter the exchange file. xplan keeps ownership of mission state. These exclusions preserve independent authorities; no repository's task-domain data is excluded.

Use one optional .switchbard/tasks.json file per repo: deterministic, versioned, and lossless. Explicit export updates it. Ordinary task mutations do not. Explicit import previews incoming changes and conflicts, then applies a reviewed plan transactionally. No automatic commit, PR, fetch, push, or import on checkout. The file is a round-trip exchange artifact, not merely a summary and not the live local authority.

## Invariants

1. Database records are live authority after per-repo cutover. Repo files become legacy inputs or explicit exchange artifacts.
2. Linked worktrees resolve to one stable repository identity. Paths, names, remotes, and task display numbers are mutable locators or labels.
3. Same-number tasks in different repos stay distinct. Forks are never linked automatically by name or remote.
4. One core command commits canonical records, indices, related changes, change sequence, and retry receipts together or not at all.
5. Preserve original bytes and provenance, including unknown frontmatter, custom sections, formatting, goals, and ranks. Do not import from the lossy parsed task struct alone.
6. An omitted incoming record never means deletion. Deletion requires an explicit tombstone; archive/completed are retained lifecycle states.
7. No timestamp-based last-writer-wins. Conflicts preserve local, base, and incoming content for explicit resolution.
8. Stale writes and mismatched command replays reject without side effects. Identical retries cannot duplicate tasks, notes, or check-ins.
9. Database failure never falls back to writing Markdown or masquerades as an empty repo.
10. Worktree removal, checkout, and repo rename do not remove or roll back task truth. A per-repo exchange is not a full database backup.

## Alternatives and tradeoffs

A central cache retains file authority and fails the owner outcome. A shared Markdown directory removes some duplication but needs a new multi-record transaction protocol. SQLite fits the required atomic boundary. A hosted service adds accounts and network operations unnecessary for Git-based exchange. A committed binary database makes review and compatibility depend on database internals. Automatic export recreates Git churn; automatic import makes historical checkouts mutate current planning state.

Use whole-record three-way merge initially: disjoint record changes merge, identical edits replay, and divergent edits to the same record need explicit resolution. Field-level merging is deferred because custom Markdown, ordered lists, goals, and rank have different semantics. This is a proposed engineering default, not a previously expressed owner preference.

## Approval boundary

The first production schema/dependency change, real database import, authority cutover, and legacy-file removal follow review of the complete contract. Synthetic temporary databases are decision evidence only. A one-time legacy cleanup may need a PR; normal future edits do not.
