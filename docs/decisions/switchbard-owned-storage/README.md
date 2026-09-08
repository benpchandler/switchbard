# Switchbard-owned storage (TASK-147)

Status: discovery in progress; storage scope and repo-summary purpose await owner input. This is not an approved implementation contract.

## Objective ledger

The owner selected TASK-147 next on 2026-09-08: Switchbard persists data instead of repository files, possibly leaving a summary in the repo. Preserve that outcome across discovery and implementation; a decision document alone does not complete the task.

Current acceptance is only reporter confirmation. Refine it into concrete storage, migration, compatibility, recovery, and summary behavior once the intended boundary is known.

## Conservation rule

Preserve all original records, including unknown frontmatter and custom Markdown sections, all task identities and relationships, ranks, goal history, and uncommitted differences across worktrees. Do not silently choose between divergent copies or treat a generated summary as a second writable authority.

## Authorization and boundaries

Authorized now: discovery, repository audit, reversible planning, task tracking. No production persistence changes, data imports, cutover, removal of backlog files, or shared process restarts have occurred. The first storage migration or authority cutover must follow an explicit, reviewable contract and owner authorization.

## Sequence

1. Inspect current storage owners and consumers.
2. Resolve what all data means and what the repo summary is for.
3. Model alternatives, stable identity, migration conflicts, concurrency, backups, and rollback.
4. Produce and independently audit the executable implementation contract.
5. Implement the agreed contract, verify real transitions, and obtain reporter confirmation.

## Open decisions

- Whether the primary outcome is central task storage with repo summaries, removing task records from Git/worktree conflicts, or consolidating all Switchbard-managed records.
- Whether a repo summary is for human reading, agent context, portability, or some combination.
- Whether independent clones and other machines must share changes in the first implementation.

## Current evidence

- `backlog/tasks/task-147 - sbt-idea-switchbard-persists-all-data-rather-than-in-repo;-maybe-writes-some-kind-of-summary-file-in-repo.md`: original owner idea and reporter-confirmation criterion.
- `crates/switchbard-core/src/backlog/mod.rs`: shared native task read/write facade.
- `crates/switchbard-core/src/backlog/types.rs:151` and `:243`: repo and task models carry filesystem paths.
- `crates/switchbard-core/src/backlog/allocate.rs`: task allocation scans worktrees and active branches, with shared Git-directory reservations.
- `crates/switchbard-task/src/main.rs:365`: CLI rejects scopes without a backlog directory.
- `crates/switchbard-tui/src/main.rs:82`: TUI startup also requires a backlog directory.
- `docs/decisions/task-queue-authority-model/decision.md`: existing contract defines task identity by repo root and task ID and promises migration-free repo files. A new contract must explicitly supersede affected clauses.

## Validation state

Read-only source inspection only. No tests run, synthetic scenarios executed, or migration behavior proved yet. Existing primary-checkout changes are preserved; decision artifacts are isolated on `codex/task-147-storage-contract`.
