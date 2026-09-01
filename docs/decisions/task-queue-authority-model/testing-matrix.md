# Testing matrix: Task Queue authority and identity model

| Invariant or risk | Valid synthetic case | Invalid or mutation case | Evidence layer | Acceptance ID |
|---|---|---|---|---|
| Native task, source binding, Project membership, GitHub artifact, link, and projection identities do not collapse | `VALID-001`, `VALID-002` | `INVALID-001`, `INVALID-013` | Pure domain serialization and projection tests | MUST-001 |
| GitHub observation has no local or remote mutation authority | `VALID-001` | `INVALID-009` | Port-shape compile test plus adapter integration spy | MUST-002 |
| Binding selection and order round-trip as explicit Switchbard intent | `VALID-001` | `INVALID-002`, `INVALID-003` | Config parse/write compatibility tests plus the GUI settings round-trip journey | MUST-003, MUST-017 |
| One local selector preserves `load_backlog_repo` stack rank for GUI, CLI, and fallback dispatch | `VALID-001` | `INVALID-010` | Core unit tests plus `sb queue` and GUI consumer tests | MUST-004 |
| Project bands and within-Project order are deterministic; duplicate artifacts render once with all memberships | `VALID-001`, `VALID-002` | `INVALID-010` | Projection property tests | MUST-005 |
| Known, Unknown, Fresh, Stale, Unavailable, partial, and MissingOrInaccessible remain distinguishable | `VALID-005`, `VALID-006` | `INVALID-005`, `INVALID-006` | Observation-state unit and adapter fixture tests | MUST-006 |
| Only the newest refresh generation publishes and disposable cache input is bounded and isolated | `VALID-005`, `VALID-008` | `INVALID-004`, `INVALID-011` | Generation-race tests plus cache fixture tests | MUST-007 |
| URL references resolve to generic links and adoption is race-safe without guessed joins, typed kinds, shadow tasks, or automatic URL rewrites | `VALID-003`, `VALID-004`, `VALID-010` | `INVALID-001`, `INVALID-013`, `INVALID-015` | Resolver, reservation, replay, and task-byte regression tests | MUST-008 |
| Dispatch success is one atomic, idempotent task-file replacement including the PR reference and normalized optional note | `VALID-007` | `INVALID-007`, `INVALID-008` | Write-failure injection, optional-note, and replay tests | MUST-009 |
| GitHub delivery evidence never implies local Done or accepted outcome | `VALID-003` | `INVALID-012` | Domain and end-to-end status tests | MUST-010 |
| Adapter requests opaque new-format ids, follows the 65-request/500-item/20-enrichment budget, and preserves failure taxonomy | `VALID-001`, `VALID-006`, `VALID-011` | `INVALID-005`, `INVALID-006`, `INVALID-011`, `INVALID-016` | Recorded GraphQL fixtures and transport-budget tests | MUST-011 |
| Refresh is off the render path and UI exposes honest loading, stale, empty, partial, error, long-list, duplicate-membership, linked, adopted, and separate dispatch-history states | `VALID-001`, `VALID-002`, `VALID-005`, `VALID-006`, `VALID-010` | `INVALID-006`, `INVALID-011`, `INVALID-015` | GUI state tests, headless journeys, and two-canonical visual approval evidence | MUST-012, MUST-016 |
| Five 100-item pages and 100-row disclosure keep refresh and render work bounded | `VALID-008` | `INVALID-011` | Benchmark fixture and threshold assertion | MUST-013 |
| A live read of the configured Lucella Project proves source identity, membership, order, provenance, and no-write behavior | Live-only fixture declared in `VALID-009` | Missing auth or scope is External Block, never synthetic success | Authenticated read-only probe with redacted evidence | MUST-014 |
| Existing task bytes and configs remain compatible and rollback loses no native work | `VALID-004`, `VALID-008` | `INVALID-014` | Golden task/config round-trip and rollback tests | MUST-015 |

## Baseline separation

`mise run ci` is the normal repository gate and must remain green throughout contract work. `node docs/decisions/task-queue-authority-model/verify.mjs` is the decision-specific implementation verifier and must remain RED until TASK-80.3 and TASK-80.4 supply the behavior each failing criterion names. Contract structure, JSON parsing, lints, and synthetic-fixture self-consistency may pass before production implementation; a proxy such as file existence must not turn a behavior criterion green.

## External evidence

MUST-014 requires a live authenticated, read-only Lucella GitHub Project probe and may be External Block only when credentials, scopes, the Project, or GitHub itself are unavailable to every agent in the run. Missing Rust modules, missing test helpers, a RED verifier, or an unimplemented UI are implementation failures, not external blocks. Visual approval for the final IA V2 surface remains a separate human gate under TASK-80.4; automated clean state is evidence, not approval.
