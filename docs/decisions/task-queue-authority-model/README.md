# Task Queue authority and identity model

Status: owner-approved data contract; production implementation is not approved or implemented.

Objective: Lock a migration-free, read-only GitHub delivery model that preserves one authority per field and produces one deterministic Task Queue without duplicate task records.

Created and approved: 2026-09-01

## Package

- `decision.md` is the governing product and authority decision.
- `architecture.md` defines identities, source bands, observation states, commands, persistence, and rollback.
- `testing-matrix.md` maps invariants and risks to executable evidence.
- `blast-radius.json` records current and proposed consumers with repository evidence.
- `synthetic-model.json` and `synthetic-invalid-cases.json` pressure-test valid composition and rejection.
- `plan.md` sequences implementation after separate approval.
- `acceptance.md` defines the implementation completion contract.
- `verify.mjs` is the one decision-specific verifier. It is expected to remain RED before TASK-80.3 and TASK-80.4 implementation.
- `audit.md` records the independent fidelity audit.

## Conservation rule

This folder is the one authority for the Task Queue GitHub data contract. The former owner-review draft at `docs/task-queue-authority-model.md` was replaced by this audited package so no competing contract can silently diverge. `docs/product-trajectory.md` and downstream tasks link here. Production code and ordinary tests stay in their conventional crate locations and link back here.

Normal repository gates must remain green while `verify.mjs` reports only genuinely unimplemented behavior as RED. A green decision package does not mean the GitHub-aware Task Queue is implemented, shipped, or approved visually.
