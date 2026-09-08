# Switchbard-owned storage (TASK-147)

Owner clarification (2026-09-08): [schema flexibility](schema-flexibility.md) is a governing requirement. Use a stable envelope with extensible content, preserve unknown fields and kinds, and avoid schema migrations for custom fields. It extends MUST-004 and MUST-017. Earlier wire/schema/model details require revision where inconsistent; implementation readiness remains open.

Status: revision required following the fresh [second opinion](second-opinion.md). Central SQLite authority is supported; exchange ancestry, record granularity, PR readability, and verification require revision before implementation. The earlier audit remains historical evidence of its narrower finding closure. Implementation, migration and reporter acceptance remain open.

## Objective ledger

The owner selected TASK-147 next on 2026-09-08 and clarified that Switchbard owns one central database across repositories, with one optional file per repo for collaboration and sync through Git. Preserve that outcome across discovery and implementation; a decision document alone does not complete the task.

The primary checkout's TASK-147 now carries acceptance for shared central records, optional one-file exchange, conflict safety, lossless migration/recovery, supported consumers, and reporter confirmation. Its worktree copy still reflects the older task; primary task edits were made through sb and remain separate from these decision artifacts.

## Conservation rule

Preserve all original records, including unknown frontmatter and custom Markdown sections, all task identities and relationships, ranks, goal history, and uncommitted differences across worktrees. Do not silently choose between divergent copies or treat a generated summary as a second writable authority.

## Authorization and boundaries

Authorized now: discovery, repository audit, reversible planning, task tracking. No production persistence changes, data imports, cutover, removal of backlog files, or shared process restarts have occurred. The first storage migration or authority cutover must follow an explicit, reviewable contract and owner authorization.

## Sequence

1. Completed: inspect current storage owners and consumers.
2. Completed: obtain owner clarification of central authority and optional Git exchange.
3. Completed at synthetic scope: model shared records, merge/replay, bootstrap and conflicts. Full product proof remains open.
4. Drafted at decision scope: executable implementation contract and initial audit closure. Fresh second opinion reopened material design and verification findings; revise before implementation.
5. Implement the agreed contract, verify real transitions, and obtain reporter confirmation.

## Owner clarification

Switchbard owns, uses, and updates one central database for all repositories. Local worktrees share that database and no longer need task PRs to synchronize ordinary updates. A repository can optionally carry one file that is committed and PR-ed for collaboration and sync. That file is an exchange artifact, not merely a prose summary, and not the live source of local worktree state.

## Proposed defaults to pressure-test

Use a machine-local SQLite database for native task-domain records across all repos. Preserve repo association while giving it a stable identity independent of checkout path. Use an explicit, deterministic per-repo JSON exchange file; do not rewrite it on every task mutation. Import previews differences and conflicts before applying them transactionally. Never infer deletes from absent records. Runtime claims remain machine-specific and xplan retains ownership of mission state. These are implementation proposals derived from the owner intent, not additional owner decisions.

## Current evidence

- `backlog/tasks/task-147 - sbt-idea-switchbard-persists-all-data-rather-than-in-repo;-maybe-writes-some-kind-of-summary-file-in-repo.md`: original owner idea and reporter-confirmation criterion.
- `crates/switchbard-core/src/backlog/mod.rs`: shared native task read/write facade.
- `crates/switchbard-core/src/backlog/types.rs:151` and `:243`: repo and task models carry filesystem paths.
- `crates/switchbard-core/src/backlog/allocate.rs`: task allocation scans worktrees and active branches, with shared Git-directory reservations.
- `crates/switchbard-task/src/main.rs:365`: CLI rejects scopes without a backlog directory.
- `crates/switchbard-tui/src/main.rs:82`: TUI startup also requires a backlog directory.
- `docs/decisions/task-queue-authority-model/decision.md`: existing contract defines task identity by repo root and task ID and promises migration-free repo files. A new contract must explicitly supersede affected clauses.

## Validation state

Synthetic model: 34/34 scenarios pass; 22 invalid cases reject without modeled durable effects. Fixed wire vectors pass 30/30 and verifier selftests pass 11/11. Limits and omissions are explicit in their result files. Plan lint, Rust formatting, Git-hook and CI-routing contract checks pass. All 34 product criteria remain RED because storage commands do not exist. No production migration behavior or GUI/TUI storage journey is proved. Existing primary-checkout changes are preserved; decision artifacts are isolated on codex/task-147-storage-contract.

## Package map

- [Decision](decision.md), [architecture](architecture.md), [source inventory](discovery.md), and [blast radius](blast-radius.json).
- [Implementation plan](plan.md), [acceptance](acceptance.md), and [state/evidence matrix](testing-matrix.md).
- [Wire specification](exchange-v1.md), [closed schema](exchange-v1.schema.json), and [fixed vectors](exchange-v1-vectors.json).
- [Initial independent audit and scope limits](audit.md), superseded readiness verdict in the [fresh second opinion](second-opinion.md).

Run the single product acceptance command with `python3 docs/decisions/switchbard-owned-storage/verify.py`. A failure is expected until the actual product behavior is implemented; the synthetic scripts are separate decision evidence and cannot turn it green.
