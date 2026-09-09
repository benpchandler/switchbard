# TASK-147 migration objective ledger

The owner authorized implementation and gradual strangler migration on 2026-09-08, once the design is satisfactory. Start with lower-impact Markdown data, verify the same observable state or documented intentional changes, and progress through all native task-domain data. Central SQLite remains shared across repositories/worktrees; one optional readable file supports collaboration; flexible custom content is a governing requirement.

## Sequence and acceptance

1. TASK-201: flexible document store and per-kind authority; initiative definitions, then project definitions. Compare legacy and central reads, exercise central writes with originals unchanged, prove concurrency and unknown-content fidelity.
2. TASK-202: configuration, ranking, goals, then tasks across all lifecycles and every consumer. Keep exactly one writer per activated kind; no dual-write ambiguity or DB-error fallback. Compare fixture and real data before cutover and expected deltas after edits.
3. TASK-203: correct optional single-file exchange, offline peer journeys, backup/restore and whole-system migration proof.

Task records are maintained through sb in the primary checkout. Isolated implementation is on codex/task-147-storage-contract. Existing primary-checkout work remains untouched except authorized tracker updates.

## Boundaries and current state

Implementation and gradual migration are authorized. Preserve legacy originals and migration provenance; deletion of originals is not necessary to end their live authority. No running shared app/process has been restarted. Coordinate any required restart before doing it. No actual native planning data has yet been migrated. Current work is contract repair and isolated implementation; no slice, test pass, commit, or PR alone completes the owner outcome.

## Implemented and checked in isolated environments

- Flexible raw-document SQLite store, stable repository/record IDs, per-kind authority, transaction rollback, revision checks, bounded causal exchange, and preserved source provenance.
- Native initiative/project/config/ranking/goals/task adapters and all task lifecycles. Cross-kind rename/reparent commits atomically once all involved kinds are central.
- CLI preview/digest/apply, private verified backups, immutable-source restore with replica rotation, explicit rebind, flexible raw document editing, workspace ordering, and conflict resolution files.
- GUI stable task identity, dirty-draft protection, external revision detection and explicit reload; TUI sequence polling and selection retention across reparenting. Focused frontend tests and state matrix recorded separately.
- Seven real repositories rehearsed through eligible phases in one temporary shared database using an immutable release binary. Task, project, initiative, and goal list output matched before/after in every case; every inventoried original source remained byte-for-byte unchanged. The release rehearsal reported no verification failures. Six conflicting kind migrations remained held.

## Open acceptance gaps

The complete workspace preflight passed (format, warning-denied Clippy, all enabled tests, and developer gates). Independent re-review closed both high findings: deletion detection and unsupported content-version mutation. Managed validation run `01M21WK0GYEQR173C20RWFM857` passed review, tests, documentation, and the full preflight with an isolated Cargo target; publishing and CI were explicitly skipped. Its documentation commit was recovered through guarded custody return at `d763eb87`. A separate public Refine journey subsequently proved that a stale refinement can overwrite a newer central edit; that regression and the captured-before-cutover variant are fixed, with 35 Refine tests, warning-denied core Clippy, and before/after public process evidence. A legacy write concurrent with the exact authority switch is not covered by an atomic cross-filesystem/SQLite barrier; rollout must quiesce legacy writers during activation. Follow-up managed validation remains pending. Real default-database activation has not occurred. Live clients must be updated before authority switches. Read-only worktree inventory found 389 differing locators across five repositories; ancestry-based classification is distinguishing stale copies from changes requiring reconciliation. Unresolved choices must not be silently applied. The current acceptance coverage map explicitly retains broader unproved cases and reporter/live gaps.

No task is marked complete solely on these slice results. Runtime implementation is committed at `6092582`; newer primary-checkout changes were preserved through merge `1571100a`. No push or PR has occurred. Repository-specific evidence and task diffs are retained privately in `~/.switchbard/migration-reviews/task147-20260908/`, outside the code deliverable.
