# Acceptance Contract: switchbard-owned-storage

Owner clarification (2026-09-08): [schema flexibility](schema-flexibility.md) is a governing requirement. Use a stable envelope with extensible content, preserve unknown fields and kinds, and avoid schema migrations for custom fields. It extends MUST-004 and MUST-017. Earlier wire/schema/model details require revision where inconsistent; implementation readiness remains open.

## Source Plan

[plan.md](plan.md), with [decision.md](decision.md), [architecture.md](architecture.md), [testing-matrix.md](testing-matrix.md), and [blast-radius.json](blast-radius.json). These are proposed implementation defaults, not production-cutover approval. Task 147 remains incomplete while product criteria are RED.

## User-Added Scope

> what i mean is a central database that switchbard uses, owns, updates, etc. for all repositories and then a single file that can go in and be Pr'ed within a repo if needed for collaboration and sync purposess vs today's state, where every time something updates it requires a PR to keep things in sync across worktrees.

## Context Read

Opened the five source documents above, discovery.md, repo CLAUDE.md, shared session-mechanics.md, docs/product-trajectory.md, crates/switchbard-task/Cargo.toml and tests/cli.rs, core tests/write_layer_real_files.rs and tests/backlog_mutations.rs, and TUI tests/harness/mod.rs. The blast-radius artifact is JSON, not a missing Markdown dependency. Existing real CLI/raw corpus/TUI harnesses provide reusable substrates; new product feature tests are not yet implemented.

## MUST Pass

- [ ] MUST-001 [behavior] All native domain records and workspace ordering persist across two repositories without backlog files. Tasks in active/completed/draft/archive states, project/initiative definitions, goals/check-ins, per-repo rank, task configuration including status vocabulary/prefix reads, writes and status upgrades through the central command layer, relationships and raw provenance reload from one machine-local SQLite store without GUI/daemon/network. Task config has at most one record per scope: kind=config, public_id=task-config, lifecycle active and never tombstoned. Absence means configured_statuses=[] and prefix TASK; reset writes the default payload. Migration/bootstrap and configured prefix/status references validate this singular record. Workspace-wide hub ordering is preserved exactly with known entries bound to stable IDs across repository rebind/task reparent, unresolved locators explicit, and remains local. (source: user; plan sections 1, 3-6) Check: `core::all_domain_records_reload`.

- [ ] MUST-002 [behavior] Linked worktrees share one scope while independent repos and forks remain separate. Database bindings alone own scope identity; Config keeps repository tracking, selection and display aliases without a competing scope authority. Exercise common Git directory resolution, same-number tasks across repos, explicit clone binding, uncertain move rebind and fork isolation; names/remotes cannot silently bind identity. Explicit scope binding supports non-Git repositories without requiring a fabricated common Git directory. (source: user; plan sections 1, 3-6) Check: `core::repository_identity_and_explicit_binding`.

- [ ] MUST-003 [behavior] Public locators remain compatible and RecordId survives hierarchy changes. Keep configured prefixes, child numbering, CLI --repo/path and display-ID selection, public locator uniqueness scoped to (RepositoryId, kind, public_id) with cross-kind same names permitted, UUID RecordId across parent/locator changes, known triage ordering identity across rebind/reparent, historical run locators, runtime BacklogTaskKey/selection/drafts/locks/task maps/cache keyed by stable repository and record IDs, and computed hierarchy/goal/rank semantics. (source: plan sections 3-6; architecture.md; testing-matrix.md) Check: `core::stable_identity_and_locators`.

- [ ] MUST-004 [behavior] Raw legacy bytes and unresolved optional memberships survive migration and edits. Byte-compare unknown frontmatter, custom Markdown sections, Unicode/multiline/unbroken text, YAML goals/rank/config and surgical edits; unresolved optional membership preserves original name/scope without invented definitions. (source: plan sections 3-6; architecture.md; testing-matrix.md) Check: `core::lossless_raw_records_and_optional_memberships`.

- [ ] MUST-005 [behavior] Whole multi-record command commits or rolls back across processes. Two real independent processes and injected mid-command failure prove canonical raw records, indices, related lifecycle/rank/goal changes, sequences and receipts commit together or remain unchanged. (source: plan sections 3-6; architecture.md; testing-matrix.md) Check: `core::multiprocess_atomic_commands`.

- [ ] MUST-006 [behavior] Stale revisions reject and exact replay is idempotent while mismatched replay rejects. Measure stale rejection with no effects, same command_id plus same arguments returns original result, different arguments reject, no duplicated tasks/notes/check-ins. (source: plan sections 3-6; architecture.md; testing-matrix.md) Check: `core::revision_and_command_receipts`.

- [ ] MUST-007 [contract] Illegal scoped record graphs reject without side effects. Execute real transactions for duplicate repo/record/display IDs, cross-scope required links, invalid kinds/lifecycle/status, required dangling links, parent/dependency cycles and referenced deletion without a validated related-change plan. (source: plan sections 3-6; architecture.md; testing-matrix.md) Check: `core::graph_constraints`.

- [ ] MUST-008 [quality] Busy handling terminates within five seconds with retry feedback. Measure wall-clock bound <=5 seconds under an actual competing database lock and assert actionable retry feedback. (source: plan sections 3-6; architecture.md; testing-matrix.md) Check: `core::busy_timeout_five_seconds`.

- [ ] MUST-009 [behavior] Migration inventories lifecycle, sibling and branch sources with explicit reconciliation. Inventory all four lifecycle sources, sibling worktrees and allocator-covered active branches with paths/Git revisions/digests. Collapse identical copies; divergent same-ID and unmerged sources require explicit reconciliation, never silent selection. (source: plan sections 3-6; architecture.md; testing-matrix.md) Check: `core::migration_inventory_and_divergence`.

- [ ] MUST-010 [behavior] Backup precedes atomic migration activation and source changes abort safely. Capture recoverable raw backup/manifest before apply, quiesce known writers, recheck all source digests and activate binding in the import transaction. Changed sources or transaction failure retains legacy authority and all source files. (source: plan sections 3-6; architecture.md; testing-matrix.md) Check: `core::migration_cutover_and_source_recheck`.

- [ ] MUST-011 [behavior] Updated clients never write legacy files and report later legacy divergence. Post-cutover updated writers do not touch legacy data; later file divergence reports without auto-import. Test old-file changes as external divergence; document unsupported old binaries without pretending a marker controls them. (source: plan sections 3-6; architecture.md; testing-matrix.md) Check: `journey::legacy_cutover_no_fallback`.

- [ ] MUST-012 [behavior] Normal domain edits change no repo files in clean or already dirty worktrees. Compare repository file bytes and Git status before/after ordinary create/edit/lifecycle/hierarchy/goal/rank/config/status-upgrade commands in clean and already-dirty linked worktrees. No automatic import/export/commit/PR/fetch/push on edit or checkout. (source: user; plan sections 1, 3-6) Check: `journey::normal_edits_no_git_churn`.

- [ ] MUST-013 [behavior] All consumers and headless queue agree after worktree removal. Exercise GUI/TUI/CLI/queue/dispatch/refine/headless consumer seams, registered startup with no backlog directory, cross-repo order, worktree deletion and app restart. Preserve sb queue/Python reconciliation and separate process claims, execution paths and xplan authority. Dispatch artifacts use RunId + RepositoryId + RecordId so same-second same-display-ID runs across repositories cannot collide; ambiguous legacy matches never authorize killing a process. (source: plan sections 3-6; architecture.md; testing-matrix.md) Check: `journey::consumer_and_queue_conservation`.

- [ ] MUST-014 [behavior] Refine applies versioned raw content centrally and rejects stale results. Drive real refine raw-document view plus expected revision into the shared command facade, preserve unknown bytes and reject stale apply without discarding source content. (source: plan sections 3-6; architecture.md; testing-matrix.md) Check: `core::refine_versioned_raw_apply`.

- [ ] MUST-015 [quality] Active GUI and TUI see commits within two seconds with at most one poll per second. Two actual active native clients observe another process commit <=2000 ms; instrument change-sequence polling <=1/s and confirm reads/projected refresh run outside input/render threads. CLI sees commits on next read. (source: plan sections 3-6; architecture.md; testing-matrix.md) Check: `gui::storage_acceptance::active_client_visibility`.

- [ ] MUST-016 [behavior] Refocus refreshes before writes and concurrent-edit conflicts retain drafts. Real GUI journey refreshes on refocus before enabling writes; concurrent save conflict keeps the unsaved draft and presents retry/resolution. During repository rebind and task reparent while selected or with a pending draft, retain stable selection/draft/lock/cache identity and prove final apply changes the same RecordId. TUI equivalent is MUST-028. (source: plan sections 3-6; architecture.md; testing-matrix.md) Check: `gui::storage_acceptance::refocus_and_drafts`.

- [ ] MUST-017 [contract] One deterministic v1 file round-trips every native record and excludes machine authorities. Round-trip optional .switchbard/tasks.json against normative exchange-v1.md, exchange-v1.schema.json and golden exchange-v1-vectors.json: v1/RepositoryId/digests/current/tombstones/at-most-one-complete-base, canonical key and RecordId order, base64 raw bytes including preserved config/status vocabulary/prefix values and upgrades that reload centrally after round-trip. Enforce the singular active, non-tombstoned config/task-config record; absent config gives configured_statuses=[] and prefix TASK, and reset writes a default payload. When base contains task-config, current must retain the same config RecordId; reject base-present/current-absent and replacement-UUID snapshots because config deletion/recreation is forbidden. Exclude machine paths, process IDs, secrets, executable views, xplan state and workspace-wide order; per-repo rank remains included. User prose is retained. (source: user; plan sections 1, 3-6) Check: `exchange::single_file_lossless_envelope`.

- [ ] MUST-018 [behavior] Export uses a consistent snapshot and atomic replacement with recoverable checkpoint ordering. Concurrent mutation during export cannot tear the snapshot. Inject temp-write/rename/checkpoint failures, record checkpoint only after replacement and reconcile ambiguous completion by digest. (source: plan sections 3-6; architecture.md; testing-matrix.md) Check: `exchange::atomic_export_and_checkpoint_recovery`.

- [ ] MUST-019 [behavior] Independent target edits are protected and unchanged repeat export is byte-identical. Repeat unchanged current/base export yields identical bytes without fresh timestamps or advancing base; independently edited target refuses overwrite pending import/reconciliation or explicit replacement, while dirty Git alone is accepted. (source: plan sections 3-6; architecture.md; testing-matrix.md) Check: `exchange::export_protection_and_idempotence`.

- [ ] MUST-020 [behavior] Explicit empty bound scope bootstraps while populated scope requires trusted ancestry. Fresh explicitly bound empty scope accepts a valid complete snapshot and stores checkpoint; never overwrite populated records. Known base must match retained local checkpoint; untrusted embedded base alone cannot establish acceptance. (source: plan sections 3-6; architecture.md; testing-matrix.md) Check: `exchange::bootstrap_and_known_base`.

- [ ] MUST-021 [behavior] Three-way merge preserves omissions and detects same-record, locator and tombstone conflicts. Real import exercises L=I no-op, L=B incoming, I=B local, disjoint merge, divergent conflict, omission retains local, explicit tombstone versus local edit conflict, and distinct RecordIds with the same (RepositoryId, kind, public_id) conflict; identical public names across different kinds remain legal. No timestamp winner. (source: plan sections 3-6; architecture.md; testing-matrix.md) Check: `exchange::three_way_merge_and_tombstones`.

- [ ] MUST-022 [behavior] Preview apply rechecks file digest and repository revision before atomic import. Persist bounded preview with expected repo revision/source digest, reject changed input or concurrent edit, and commit records/provenance/checkpoint/receipt atomically on explicit apply only. (source: plan sections 3-6; architecture.md; testing-matrix.md) Check: `exchange::stale_preview_and_atomic_apply`.

- [ ] MUST-023 [behavior] Explicit resolution preserves originals and validates the complete resulting graph. Choose local/incoming/custom, retain all local/base/incoming originals, validate entire merged graph, and prove unresolved conflict leaves live records unchanged. (source: plan sections 3-6; architecture.md; testing-matrix.md) Check: `exchange::resolution_graph_validation`.

- [ ] MUST-024 [contract] Input boundary limits and malformed or untrusted documents reject without live effects. Accept exactly 64 MiB total, 100,000 records TOTAL across base plus current, 4 MiB decoded record payload, depth 32; reject one over each. Reject duplicate JSON keys at every nesting level before typed parsing; reject unsupported version, malformed JSON/Git markers, count/digest mismatch, invalid IDs/enums/links/cycles/wrong scope, multiple or malformed config/task-config records, config tombstones/inactive lifecycle, base-present/current-absent config and replacement config UUID when the base contains task-config, and invalid configured prefix/status references; never truncate, execute imported config or partially import. (source: plan sections 3-6; architecture.md; testing-matrix.md) Check: `exchange::untrusted_input_limits`.

- [ ] MUST-025 [behavior] Canonicalization repairs format without importing or trusting unknown ancestry. Malformed/manual edits receive actionable validate/canonicalize instruction; repaired digest does not import, approve apply or make unknown ancestry trusted. (source: plan sections 3-6; architecture.md; testing-matrix.md) Check: `exchange::canonicalize_not_import`.

- [ ] MUST-026 [behavior] Unavailable or corrupt database reports failure without empty success or Markdown fallback. Inject unavailable/corrupt/locked storage and inspect error plus zero legacy filesystem writes; failure must never masquerade as an empty repository. (source: plan sections 3-6; architecture.md; testing-matrix.md) Check: `journey::database_failure_is_explicit`.

- [ ] MUST-027 [behavior] Database-consistent restore retains post-cutover writes and reverse migration reconciles newer data. Restore a database-consistent backup into a temporary store and verify post-cutover writes/integrity/revisions. Reverse migration needs lossless newer-data reconciliation and explicit reactivation; a per-repo exchange or stale-file copy/code revert is not full recovery. (source: plan sections 3-6; architecture.md; testing-matrix.md) Check: `core::backup_restore_and_reverse_migration`.

- [ ] MUST-028 [behavior] TUI real-key state journeys preserve drafts and existing legacy fixture behavior. Real TUI input/render harness with temporary central DB exercises keyboard-only preview/conflict navigation, focus/scroll, saving/failure/retry, retained drafts, refocus and legacy fixtures. (source: plan sections 3-6; architecture.md; testing-matrix.md) Check: `tui::storage_state_and_keyboard_journey`.

- [ ] MUST-029 [visual] GUI storage states render in narrow current and wide containers against an approved canonical. Render actual GUI for every applicable state in testing-matrix.md in narrow/current/wide containers. Canonical storage-state design is currently missing; named runtime render test plus hashed PNGs and approved canonical required, no model/browser mock substitution. (source: plan sections 3-6; architecture.md; testing-matrix.md) Check: `gui::storage_acceptance::visual_state_matrix`.

- [ ] MUST-030 [visual] TUI storage states render at 80x24 and current size against an approved canonical. Render actual TUI for every applicable state in testing-matrix.md at 80x24/current size with approved canonical and hashed real renders; no screenshot existence-only success. (source: plan sections 3-6; architecture.md; testing-matrix.md) Check: `tui::storage_visual_state_matrix`.

- [ ] MUST-031 [quality] Integrated implementation passes platform gates and render performance comparison. Run mise run preflight, macOS/Linux CI, uv run --directory orchestrator pytest, and actual SWITCHBARD_PERF smoke with log override comparing frame/workspace p95 to preceding build for changed GUI paths. Execute local preflight and orchestrator commands, query exact-source CI job conclusions, and recompute captured native before/after performance CSVs with scripts/perf-summary.py. Missing external run or captures fails closed; JSON pass strings cannot satisfy it. (source: plan sections 3-6; architecture.md; testing-matrix.md) Check: `evidence::integration_gates`.

- [ ] MUST-032 [behavior] Reporter confirms two-client no-PR workflow and independent-clone single-file exchange. Reporter source receipt confirms real two-client no-PR editing and independent clone import/edit/export/PR-file collaboration; actual timing <=2000 ms and source revision recorded. Synthetic model outcomes cannot satisfy reporter acceptance. (source: user; plan sections 1, 3-6) Check: `evidence::reporter_journey`.

- [ ] MUST-033 [contract] Authority and compatibility documentation matches tested behavior and preserves independent authorities. Review docs/product-trajectory.md, task-queue-authority-model/decision.md supersession, CLI help, module docs and crates/switchbard-tui/CLAUDE.md against tested behavior. Record old-client limitations, migration/restore rules, exclusions, explicit cleanup boundary and every remaining state gap. (source: plan sections 3-6; architecture.md; testing-matrix.md) Check: `evidence::documentation_review`.

- [ ] MUST-034 [contract] Storage files and backups preserve private ownership and permissions under permissive umask. Execute real creation/open/backup paths and assert database, sidecars and backup files are mode 0600 and newly created backup directories are mode 0700. Validate existing path ownership/privacy and reject symlink or foreign-owned substitution without writing; do not broadly chmod existing directories. (source: architecture.md storage privacy contract) Check: `core::storage_privacy_and_path_substitution`.

## SHOULD Pass / Review Manually

- Review implementation-evidence.md and state-evidence.md for clear commands, revision, platform, timings and explicit unsatisfied gates. Review all BR-01 through BR-24 dispositions, including headless startup and historical locators.
- Confirm proposed SQLite path ~/.switchbard/switchbard.sqlite3 with alternate/test root, format limits and whole-record merge defaults remain acceptable before production schema/dependency changes or migration.

## Out Of Scope

Hosted services, daemons, automatic Git/import/export operations, field-level merge, unrelated cleanup, and transferring ownership of machine preferences, executable TUI views, process liveness, caches/logs, Git observations or xplan mission writes. Workspace-wide order is central local state, not per-repository exchange content. Existing shell styling beyond new storage states is unchanged; preserving its interactions is part of consumer conservation. Production migration, authority activation on real data and legacy deletion remain separately authorized. Touch and network roles are inapplicable to this native/local workflow.

## Verifier Command

`python3 docs/decisions/switchbard-owned-storage/verify.py`

## Required Summary Output

verifier-results.json contains exactly one criteria entry per MUST, source commit/timestamp, actual workspace command/output/exit evidence, exact named test mapping, failures and mutation deferrals. Missing commands are observed FAIL, not dependency-blocked. Missing/ignored named tests and any nonzero shared runner cannot pass. Each suite executes once; each criterion reads its exact named result. The initial run invokes the actual workspace CLI under temporary HOME/XDG roots and a worktree-specific Cargo target, never installed sb. Missing CLI seams stop costly downstream suites while retaining per-MUST unsatisfied outcome evidence. This prerequisite RED is not a test of every future behavior.

## Required Completion Artifacts

- Normative exchange-v1.md, exchange-v1.schema.json, exchange_v1_vectors.py and exchange-v1-vectors.json; the Python vectors test the specification only, never product behavior.
- All modules, fixture storage-v1.json, and real product suites named in plan.md section 6; implementation-evidence.md and state-evidence.md.
- Product source-revision evidence product-evidence.json with hashed local artifacts, actual render canonicals and every required state/container PNG, reporter source receipt and explicit documentation review. Tests remain necessary for visual success.
- Production mutation-evidence.json at first GREEN: per-behavior and contract-target before PASS / mutated FAIL / restored PASS result JSON, hashed artifacts, source mutation/diff and matching revision. Never mutate the verifier as proof. Acceptance success is impossible while mutation evidence is absent.

## Verifier Gaps Queued

All 34 exact tests/reviews above are future product evidence; no existing model check substitutes for them. MUST-034 needs real filesystem privacy/substitution tests; MUST-001 through MUST-030 need implementation and named tests in the suite mapping below; MUST-029/030 also need approved real-container canonicals and full state render evidence. MUST-031 executes preflight and orchestrator gates after CLI prerequisites succeed, then needs quality-inputs.json with an exact-source CI run ID and hashed real native before/after capture CSVs, binaries and launch/build source receipts. CI is re-queried and performance metrics recomputed; absent evidence fails closed. MUST-032 needs reporter evidence. MUST-033 needs documented source review. These are implementation completion gates, not claims that decision-stage code is broken.

| Suite | One shared execution |
| --- | --- |
| core | `cargo test -p switchbard-core --test central_storage -- --color never` |
| exchange | `cargo test -p switchbard-core --test storage_exchange -- --color never` |
| journey | `cargo test -p switchbard-task --test storage_journey -- --color never` |
| tui | `cargo test -p switchbard-tui --test central_storage -- --color never` |
| gui | `cargo test -p switchbard-gui storage_acceptance:: -- --color never` |
| evidence | Exact keyed, revision-bound review artifacts; executable quality never passes from attestation |

## Kind Breakdown

- behavior: 24
- contract: 5
- quality: 3
- visual: 2
- Total: 34

## Known Risks / Open Questions

BLOCKED for visual completion: no approved canonical storage-state GUI/TUI reference exists yet. State layout is not approved by this contract. Integrated quality execution is deferred until feature CLI prerequisites exist; exact-source remote CI and coordinated native performance capture are still absent. Proposed CLI spelling uses `storage`, `storage migrate`, `storage export`, and `storage import`; exact option grammar must be defined with implementation and kept aligned with the real CLI journey tests. Current RED proves those prerequisites are absent, not each downstream safety property. A full GREEN requires real named tests, independent mutation evidence, all state coverage and reporter confirmation; the synthetic decision model cannot supply any of those.

## Quality Input Format

`quality-inputs.json` provides `commit`, numeric `ci_run_id`, and `perf.before` / `perf.after`. Each capture names its source `commit`, `scenario: servers-scroll-smoke`, `reviewer`, `env` with `SWITCHBARD_PERF=1` and the exact `SWITCHBARD_PERF_LOG`, plus `{path, sha256}` objects for `csv`, `binary`, and `source_receipt`. The receipt records source commit, binary/csv hashes, build exit, real-app flag, scenario, launch command and capture start/end. Baseline must be an earlier ancestor build. Product commands retain isolated HOME/XDG roots; the read-only GitHub query retains the original GH_CONFIG_DIR path or inherited GH_TOKEN without exposing credentials. Missing authentication fails explicitly. Local commands execute once, remote CI is queried live for the current source and all five platform jobs, and the repository perf-summary command recomputes both p95 measurements. CSV, binary, source receipt and human approval paths must resolve inside the workspace; symlink escapes are rejected. perf.human_approval is a hashed {path, sha256} artifact naming reviewer, reviewed_at, approved=true, scenario and both captures keyed before/after with commit, csv_sha256 and binary_sha256. This explicit human approval covers actual native capture; source receipts alone cannot pass. Output distinguishes measured p95 from HUMAN ATTESTATION of native capture. The contract requires comparison without inventing a percentage budget. Missing evidence cannot pass; this verifier never launches or restarts the user's shared GUI automatically.

## Verifier Logic Selftests

`python3 docs/decisions/switchbard-owned-storage/verifier_selftest.py` exercises fail-closed verifier parsing and latent evidence branches with controlled runner responses and disposable artifacts. These are tests of the acceptance harness only, never product or storage evidence.
