# TASK-147: independent second opinion

Verdict: **revise before implementation**. Keep one machine-local SQLite database owned by Switchbard across repositories. Rework the collaboration contract and record representation before fixing the production schema.

Review date: 2026-09-08. Subject: contract commit `4a50c90`. A fresh architecture advisor reviewed without inherited conversation context and was instructed to form conclusions before reading the earlier audit verdict. This report preserves its findings, with two peer scenarios independently reproduced by the coordinating agent. No production implementation, live migration, push, or PR occurred during this review.

## 1. Blocker: ordinary collaborator catch-up is not covered

[Architecture](architecture.md) advances the next export's base to the last accepted or exported snapshot and requires populated receivers to recognize that base locally. A and B share S0; A exports S1 privately, then exports S2 for sharing. B receives only S2 and gets `unknown-base`, even when B has made no changes since S0. This follows the contract, preserves data, and forces reconciliation for an ordinary offline collaborator. The contract does not deliver a simple latest-file catch-up journey.

A separate model defect occurs when B bootstraps S0, edits, and exports back to A: the model emits `base=null`, and A rejects it. `synthetic_model.py` selects export ancestry from prior exports while bootstrap stores only a checkpoint. This contradicts the prose's accepted-checkpoint rule; it is not an implemented product defect.

Both cases are reproducible with `python3 docs/decisions/switchbard-owned-storage/second_opinion_probe.py`. [Recorded results](second-opinion-probes.json) are synthetic SQLite evidence, not application E2E evidence.

Recommended revision: evaluate a current-state exchange file with a locally retained accepted sync baseline. Specify identity, first import/export, offline catch-up, divergent changes, and conflict resolution before selecting the algorithm. If retaining embedded ancestry, define a practical skipped-revision recovery journey and its cost explicitly. The alternative is a design direction requiring pressure-testing, not an approved replacement protocol.

## 2. Blocker: define record granularity per kind

The architecture preserves raw YAML for goals and ranking, but does not resolve whether an aggregate source file is one record or a set of independently identified records. Existing goals share one file (`crates/switchbard-core/src/backlog/goals.rs`); ranking contains distinct lanes (`crates/switchbard-core/src/backlog/ranking.rs`). Whole-file records cause unrelated edits to conflict. Per-goal records need a specified ownership model for source comments, ordering, and unknown fields.

Define stable identity, payload granularity, relationship ownership, deletion/tombstone semantics, and source provenance for every kind. Consider canonical structured goal/ranking records with immutable original source bytes retained as migration provenance. Preserve the user's information without assuming byte-exact legacy formatting must remain the exchange representation forever.

## 3. High: make the optional file useful in a PR

[The wire specification](exchange-v1.md) carries full base and current snapshots and base64-encodes source documents. A small prose edit becomes an opaque encoded-line replacement. Successor files duplicate much of the state, and disjoint concurrent exports both alter a shared digest, creating a Git merge hotspot. The design supports transport through Git but offers weak human review and merge ergonomics.

Prefer readable logical records in a deterministic current-state document. Keep migration originals and operational provenance inside the database and backups unless byte-exact source exchange is an explicit requirement. Validate the eventual format using actual diffs for independent task edits and concurrent PRs.

## 4. High: strengthen the acceptance evidence

The matrix covers bootstrap and repeated export/import separately, but omits the exact peer sequences above. Add named deterministic product tests for bootstrap/edit/export-back and importing the latest file after missing intermediate exports. Existing passing synthetic tests and wire vectors do not establish these journeys.

The future mutation gate in `verify.py` checks a top-level commit, artifact hashes, and asserted before/mutated/restored statuses. It does not execute mutations or independently bind nested results to revision-specific command execution. A hash establishes artifact integrity, not that a mutation ran. Execute the mutations or independently validate revision-bound commands and outputs. The current product verifier remains RED, so this is a future false-confidence risk, not a claim that current product checks passed incorrectly.

## Retain these foundations

- A single SQLite authority shared by local processes directly solves worktree synchronization without routine PRs.
- Stable repository identity, explicit clone binding, and no remote-name auto-binding protect repository boundaries.
- Transactional writes, revision checks, idempotency receipts, explicit database failures, and no Markdown fallback are sound.
- Previewed per-repository cutover, preserved originals, consistent database backups, and explicit reverse migration are sound. Reverting code is not a data rollback after new writes.
- Precise wire vectors and the explicit distinction between synthetic and product evidence should remain.

The recommendation assumes collaborators should be able to import the latest shared file after being offline and understand task changes in a PR. These assumptions follow the stated collaboration goal but are not separately approved format decisions. TASK-147 remains incomplete; the requested second opinion is complete. The normative contract is intentionally unchanged so its findings remain reviewable before revision.
