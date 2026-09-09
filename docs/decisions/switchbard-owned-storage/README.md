# Switchbard-owned storage (TASK-147)

Owner clarification (2026-09-08): [schema flexibility](schema-flexibility.md) is a governing requirement. Use a stable envelope with extensible content, preserve unknown fields and kinds, and avoid schema migrations for custom fields. It extends MUST-004 and MUST-017. Earlier wire/schema/model details require revision where inconsistent; implementation readiness remains open.

Status: owner-authorized strangler implementation in progress. The [phased contract](phased-contract.md) governs per-kind cutover and the [migration ledger](migration-ledger.md) tracks the complete outcome. Core storage, all native kinds, exchange, recovery, workspace ordering, and frontend identity/refresh paths are implemented with isolated tests. Real-data rehearsals preserve ordinary task-list output and original source bytes in six repositories. Live cutover and conflicting worktree reconciliation remain open. Earlier wire/model results are historical evidence, not implementation approval.

## Objective ledger

The owner selected TASK-147 next on 2026-09-08 and clarified that Switchbard owns one central database across repositories, with one optional file per repo for collaboration and sync through Git. Preserve that outcome across discovery and implementation; a decision document alone does not complete the task.

The primary checkout's TASK-147 now carries acceptance for shared central records, optional one-file exchange, conflict safety, lossless migration/recovery, supported consumers, and reporter confirmation. Its worktree copy still reflects the older task; primary task edits were made through sb and remain separate from these decision artifacts.

## Conservation rule

Preserve all original records, including unknown frontmatter and custom Markdown sections, all task identities and relationships, ranks, goal history, and uncommitted differences across worktrees. Do not silently choose between divergent copies or treat a generated summary as a second writable authority.

## Authorization and boundaries

Authorized now: contract revision, implementation, verification and gradual strangler migration, lower-impact records first, by owner instruction on 2026-09-08. Preserve original files and compare behavior at each stage. No live data cutover or shared process restart has yet occurred; test migrations use disposable databases.

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

- [Current product coverage and residual gaps](coverage-current.md) maps tests to each acceptance criterion without converting supporting passes into full acceptance.
- Private read-only repository previews identify eligible kinds and conflicting source copies. Operational reports are retained outside the source tree in the owner's `~/.switchbard/migration-reviews/task147-20260908/` folder.
- Private phased shadow reports record temporary shared-database migrations, ordinary view comparisons, and preserved originals; source-specific reconciliation evidence stays outside the code deliverable.
- [Current exchange contract](exchange-v2.md) supersedes the historical v1 wire/model proposal.
- [Operator guide](../../central-storage.md) explains preview, activation, exchange, ordering, and recovery commands.

## Validation state

Focused storage, native adapters, real CLI exchange/recovery/ordering journeys, and frontend stale-draft/identity tests have passed during implementation. Final integrated workspace validation and independent review remain pending. The executable verifier deliberately leaves criteria with missing live or contract evidence unproved. No default database has been migrated yet.

## Package map

- [Decision](decision.md), [architecture](architecture.md), [source inventory](discovery.md), and [blast radius](blast-radius.json).
- [Implementation plan](plan.md), [acceptance](acceptance.md), and [state/evidence matrix](testing-matrix.md).
- [Current wire specification](exchange-v2.md), [schema](exchange-v2.schema.json), and [historical evidence boundary](historical-evidence.md).
- [Initial independent audit and scope limits](audit.md), superseded readiness verdict in the [fresh second opinion](second-opinion.md).

Run the single product acceptance command with `python3 docs/decisions/switchbard-owned-storage/verify.py`. A failure is expected until the actual product behavior is implemented; the synthetic scripts are separate decision evidence and cannot turn it green.
