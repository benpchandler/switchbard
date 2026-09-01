# Acceptance Contract: task-queue-authority-model

## Source Plan

[`plan.md`](plan.md) is the authoritative implementation plan. [`decision.md`](decision.md), [`architecture.md`](architecture.md), [`testing-matrix.md`](testing-matrix.md), [`blast-radius.json`](blast-radius.json), [`synthetic-model.json`](synthetic-model.json), and [`synthetic-invalid-cases.json`](synthetic-invalid-cases.json) constrain this contract and may not be weakened by an implementation slice.

The agreed authoritative summary path is `tmp/task-queue-authority-model-acceptance.json`. It replaces the plan's provisional `tmp/task-queue-authority-model-verifier.json` filename so there is one verifier output, not two competing summaries.

## Context Read

- Repository rules and product direction: `CLAUDE.md`; `docs/product-trajectory.md`, especially Task Queue orchestration, Task Queue GitHub authority and identity, and IA V2.
- Complete decision package: `README.md`, `decision.md`, `architecture.md`, `testing-matrix.md`, `blast-radius.json`, `synthetic-model.json`, `synthetic-invalid-cases.json`, `plan.md`, and `tmp/task-queue-authority-model-plan-lint-final.json`.
- Native task and rank boundaries: `crates/switchbard-core/src/backlog/types.rs`, `ranking.rs`, `mutations.rs`, `allocate.rs`, and `write.rs`.
- Existing dispatch consumers and protocol: `crates/switchbard-core/src/dispatch.rs`, `crates/switchbard-task/src/queue_cmd.rs`, `crates/switchbard-task/tests/cli.rs`, `orchestrator/switchbard_orchestrator/proto.py`, and `orchestrator/tests/`.
- Config, GitHub-read, worker, and GUI seams: `crates/switchbard-core/src/config.rs`, `crates/switchbard-core/src/landing.rs`, `crates/switchbard-gui/src/app.rs`, `workers.rs`, `runtime/mod.rs`, `ui/settings.rs`, and `ui/places/dispatches.rs`.
- Existing verification conventions: `crates/switchbard-core/tests/backlog_mutations.rs`, `crates/switchbard-gui/tests/tasks_place_perf_smoke.rs`, and repository `scripts/verify_*.sh` files.

## MUST Pass

- [ ] MUST-001 [contract] The executed core test `must_001_distinct_identities_and_memberships` must round-trip separate native-task, source-binding, Project-membership, GitHub-node, generic-link, and queue-projection identities for `VALID-001` and `VALID-002`, retain every membership for a duplicate artifact, and reject the identity collapses in `INVALID-001` and `INVALID-013`; opaque `(host, new-format node id)` identity must never be derived from mutable coordinates.
- [ ] MUST-002 [behavior] The executed core test `must_002_refresh_has_no_mutation_capability` must drive `RefreshGitHubProject` through an adapter spy for success, partial, and failure responses and observe zero GitHub mutation calls, zero native task-writer calls, unchanged task bytes, and aborted snapshot publication for `INVALID-009`; the public observation port exposes reads only.
- [ ] MUST-003 [behavior] The executed core test `must_003_bindings_round_trip_atomically_in_order` must load an old config as an ordered empty binding list, then add, move, remove, save, and reload `github.com` Project bindings while preserving repo scope and order; duplicate locators, unsupported hosts, unknown ids, and cross-scope moves must fail with exact prior config/cache bytes and zero network requests.
- [ ] MUST-004 [behavior] The executed tests `must_004_core_selector_preserves_ranked_order`, `must_004_sb_queue_preserves_shared_order`, and `must_004_gui_uses_shared_local_order` must feed one ranked fixture through the core selector, Rust fallback dispatcher, `sb queue`, and GUI and observe identical eligible local ids in `load_backlog_repo` order with no id re-sort; remote-only rows must remain outside local rank and expedite commands until adoption.
- [ ] MUST-005 [behavior] The executed core test `must_005_source_bands_are_deterministic` must compose the local band first, configured Project bands in binding order, active Project items in membership order, closed or archived items in history, one visible row for a duplicate artifact with all memberships retained, and no standalone remote row for a resolved linked or adopted item.
- [ ] MUST-006 [contract] The executed core test `must_006_observation_states_preserve_unknowns` must validate the `Fact<T>` and source-state algebra for NeverObserved, Refreshing with prior data, Fresh, Stale, and Unavailable; a 404, null, or permission-masked result must be `MissingOrInaccessible`, rate limiting must retain `reset_at_ms`, and omitted, failed, unsupported, deferred, or truncated connections must remain Unknown rather than known-empty, Gone, or success.
- [ ] MUST-007 [behavior] The executed core test `must_007_generation_and_cache_are_bounded_and_isolated` must prove that only the newest refresh generation publishes, a later failure retains the latest successful snapshot as Stale, and each versioned/viewer-bound/binding-bound cache atomically accepts at most 500 memberships with mode 0600 where supported; wrong or corrupt entries are isolated and cache loss or removal changes no native task bytes.
- [ ] MUST-008 [behavior] The executed core integration test `must_008_links_and_adoption_are_race_safe` must resolve canonical URL references only to generic links, preserve original task URLs across repository transfer or rename, reject unresolved or conflicting identity and typed link kinds, and run two concurrent `AdoptGitHubItem` calls under the git-common-dir `(repo root, host, node id)` reservation so both return one local task whose initial revision contains one canonical reference and whose standalone remote projection is suppressed; the separately executed focused test `must_008_adoption_reservation_wrapper_is_sibling_only` must compile and exercise the narrow `pub(super) adoption_reservation_dir(repo_root)` call from the sibling adoption module, reject access outside that parent module, and prove the existing ordinary ID-reservation path remains private and behaviorally unchanged.
- [ ] MUST-009 [behavior] The executed core, CLI, and orchestrator tests `must_009_atomic_dispatch_success`, `must_009_queue_release_uses_atomic_success`, and `test_must_009_release_uses_atomic_cli_boundary` must prove that `RecordDispatchSuccess` performs one task-file replacement containing `dispatched`, `In Review`, one de-duplicated canonical PR reference, and one composite note beginning `Dispatch PR: <url>` with blank optional text normalized to none; an identical normalized replay is a no-op, a different URL or normalized note conflicts, validation/write failure preserves exact claimed-task bytes, and no caller appends after success.
- [ ] MUST-010 [behavior] The executed core and GUI tests `must_010_remote_evidence_never_completes_local` and `must_010_gui_remote_evidence_never_marks_done` must drive merged, released, deployed, successful-refresh, known-empty, and missing-looking observations against linked native tasks and observe unchanged local status, acceptance criteria, rank, claim, and outcome acceptance.
- [ ] MUST-011 [quality] The executed core test `must_011_adapter_budget_and_failure_taxonomy` must emit a measured `MUST-011` probe summary from recorded GraphQL transports showing exactly five 100-membership Project page requests for the 500-membership fixture, exactly 500 retained memberships, exactly 20 enriched artifacts selected linked-first then Project order, no more than three detail requests per enriched artifact, no more than 65 total requests and no request 66, caps of 10 closing PRs, 50 reviews, 100 commits, 100 check runs, 20 releases, and 20 deployments, and six deliberate descendant overflow plus six `hasNextPage` fixtures that produce zero descendant follow-up page requests and zero descendant connections over their caps, opaque new-format ids plus `__typename`, and explicit Unknown reasons for every unspent, incomplete, unsupported, inaccessible, rate-limited, or transport-failed field.
- [ ] MUST-012 [behavior] The executed GUI test `must_012_state_stress_and_history_conservation` must drive the real Tasks / Dispatches surface through loading, fresh, known-empty, partial, stale, unavailable, rate-limited, duplicate-membership, linked, adopted, long-title, narrow-window, and 100-of-500 disclosure states; refresh work must stay off the render thread, remote-only rows must expose no local priority/dependency/claim affordances, and the separate Active/Queued/Finished/Failed activity history must retain its kill/retry/watch/log/detail behavior and newest-run chronology without changing source-band order when run timestamps change.
- [ ] MUST-013 [quality] The executed GUI performance test `must_013_500_item_task_queue_perf_budget` must emit a measured `MUST-013` probe summary from exactly 500 observed remote items with exactly 100 disclosed rows over exactly 200 rendered frames and assert frame p95 is strictly below 40 ms.
- [ ] MUST-014 [behavior] The verifier must execute the authenticated, read-only Lucella Project 3 live probe and observe the configured binding id, Project node id, ordered ProjectV2Item and content node ids, provenance and freshness, zero GitHub mutations, and byte-identical native task/config hashes before and after at the exact clean implementation revision; only unavailable credentials, required scope, Project access, or GitHub may report an External Block, while a missing probe or implementation is a failure.
- [ ] MUST-015 [behavior] The executed core and CLI tests `must_015_compatibility_and_rollback_preserve_native_work` and `must_015_queue_protocol_remains_compatible` must load and round-trip pre-decision task/config fixtures without migration, default missing bindings to empty, preserve URL references and all native task bytes across cache deletion, source removal, and code rollback fixtures, and keep the existing `sb queue` payload and Rust/LangGraph custody contract compatible.
- [ ] MUST-016 [visual] Visual Review evidence must compare every MUST-012 state at the exact clean implementation revision against both the frozen IA V2 shell at `~/.lavish/switchbard-ia-places.html` and the owner-reviewed mixed-source body canonical at `docs/decisions/task-queue-authority-model/task-queue-visual-canonical.html`, record each canonical's exact path, SHA-256 hash, and reviewed revision, record each resolved finding against the implementation revision or a newer revision/stable URL, and include explicit human approval for the rendered Tasks / Dispatches result; a missing or unreviewed body canonical blocks TASK-80.4 UI implementation, and zero annotations, an automated clean state, a dirty render, stale canonical evidence, or evidence from another commit does not pass.
- [ ] MUST-017 [behavior] The executed GUI journey `must_017_settings_binding_controls_persist_order` must use the existing settings surface as a user to add two `github.com` Project bindings, reorder them, remove one, close and reload settings, and observe the exact persisted remaining binding and source-band order without calling a config mutation directly from the test.

## SHOULD Pass / Review Manually

- Review the redacted Lucella evidence for accidental secret, token, email, or private-content disclosure before attaching it to TASK-80.4.
- Record the machine, build profile, and comparison baseline beside the MUST-013 measurement so later 40 ms regressions can be interpreted without changing the threshold.

## Out Of Scope

- GitHub write-back, issue creation, Project field writes, PR actions, two-way synchronization, and all GitHub mutation paths.
- Remote-only per-item Switchbard rank, expedite, dependencies, claim, priority, or outcome fields before explicit local adoption.
- Cross-repo interleaving through the hub ordering overlay, GitHub Enterprise Server, other forges, webhooks, historical snapshot ledgers, and typed link kinds.
- Automatic rewriting of user-authored task references after repository transfer or rename.
- Treating merged, released, deployed, known-empty, refresh-success, or a clean automated visual state as local outcome acceptance or human approval.
- Replacing the existing Tasks / Dispatches IA route or its separate activity/history monitor.

## Verifier Command

`node docs/decisions/task-queue-authority-model/verify.mjs`

This is the one authoritative decision-specific command. `mise run ci` remains the independent repository regression floor and is not duplicated inside this verifier.

## Required Summary Output

Every run must overwrite `tmp/task-queue-authority-model-acceptance.json` with the current commit, an ISO-8601 timestamp, exactly one criterion object for each MUST-001 through MUST-017, honest kind and check type, per-test or per-probe evidence, measured metrics, required artifact state, skipped and failure arrays, and a mutation gate. Missing dependencies, test targets, named tests, metrics, probes, canonicals, or evidence must be fail or blocked and never pass or skipped.

The human summary must print each criterion status and evidence, the final PASS/PARTIAL/BLOCKED/FAIL result, the summary path, and mutation-gate deferred/probed counts. The verifier exits zero only when all seventeen criteria pass and the mutation gate has no unprobed green behavior or contract criterion.

## Required Completion Artifacts

- `crates/switchbard-core/tests/task_queue_authority_contract.rs`, containing the exact named core integration tests used by MUST-001 through MUST-011 and MUST-015, plus the focused unit test `must_008_adoption_reservation_wrapper_is_sibling_only` under `crates/switchbard-core/src/backlog/`.
- `crates/switchbard-task/tests/task_queue_authority_contract.rs`, containing the exact named CLI tests used by MUST-004, MUST-009, and MUST-015.
- `orchestrator/tests/test_task_queue_authority_contract.py`, containing the exact named LangGraph protocol test used by MUST-009.
- `crates/switchbard-gui/tests/task_queue_github_states.rs`, containing the exact named GUI tests used by MUST-004, MUST-010, MUST-012, and MUST-017.
- `crates/switchbard-gui/tests/task_queue_github_perf_smoke.rs`, containing the exact named measurement used by MUST-013.
- `scripts/probe_task_queue_lucella.mjs` and fresh, redacted `tmp/task-queue-authority-model-live-lucella.json` evidence used by MUST-014.
- Frozen IA V2 shell `~/.lavish/switchbard-ia-places.html` and owner-reviewed mixed-source body canonical `docs/decisions/task-queue-authority-model/task-queue-visual-canonical.html`, with exact path/hash/revision records in the fresh `tmp/task-queue-authority-model-visual-review.json` evidence used by MUST-016 alongside clean implementation revision, state inventory, resolution lineage, and explicit human approval.
- Fresh `tmp/task-queue-authority-model-mutations.json` evidence for every green behavior MUST and both distinct contract targets; each entry names mutated production code, the short diff, PASS-to-FAIL-to-PASS results, and the current commit.
- Fresh `tmp/task-queue-authority-model-acceptance.json`, produced only by the verifier command above.
- TASK-80.3 and TASK-80.4 completion records linked to their exact verification evidence; the parent TASK-80 remains open until all seven parent acceptance conditions are independently reconciled.

## Verifier Gaps Queued

- MUST-001 through MUST-015 and MUST-017 are intentionally RED until TASK-80.3 and TASK-80.4 add the exact named executable tests, settings journey, and probes above; missing targets or zero matching tests remain failures.
- MUST-014 may become BLOCKED only after the real probe runs and records one allowed external unavailability reason. A missing live-probe implementation remains FAIL.
- MUST-016 remains BLOCKED until the owner-reviewed mixed-source body canonical exists and exact-revision Visual Review comparison evidence plus explicit human approval exist.
- Every currently RED behavior MUST and the two distinct contract targets remain in `mutation_gate.deferred`; each moves to a current-commit flipped mutation record at its first green pass.

## Kind Breakdown

| Kind | Count |
|---|---:|
| behavior | 12 |
| contract | 2 |
| quality | 2 |
| visual | 1 |
| **Total** | **17** |

## Known Risks / Open Questions

- BLOCKED: `docs/decisions/task-queue-authority-model/task-queue-visual-canonical.html` does not yet supply the required owner-reviewed mixed-source body oracle. TASK-80.4 UI body implementation may not start until that canonical exists alongside the frozen `~/.lavish/switchbard-ia-places.html` shell reference.
- No product or authority decision remains open. Once the missing canonical is approved, visual composition remains bounded by both named canonicals, the project theme, the MUST-012 state matrix, exact-revision evidence, and human approval.
- The live Lucella gate depends on external credentials and Project access, but only the verifier's executed probe may classify that dependency as blocked.
