---
id: TASK-147
title: Central database across repositories with optional single-file Git sync
status: In Progress
assignee: []
created_date: '2026-09-03 22:32'
updated_date: '2026-09-09 02:42'
labels:
  - tui
  - idea
  - ball:agent
dependencies: []
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Switchbard should own, use and update one central database for all repositories. Local worktrees share current task data without committing, merging or opening a PR for ordinary updates. Each repository may optionally carry one file that collaborators commit and PR for explicit sync between databases. That file supports round-trip collaboration; it is not the live local source of truth.

Impact: users and agents currently need Git synchronization to carry task updates across worktrees, creating repository churn and stale or divergent task copies.

Original filing: sbt 0.4.0 at view=custom filter=empty, selected=TASK-128, pane=None. Original wording: switchbard persists all data rather than in repo; maybe writes some kind of summary file in repo? The captured screen/action trail remains below. Owner clarified the central database plus optional exchange-file outcome on 2026-09-08.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Reporter confirms the behaviour in sbt matches what they were trying to do
- [ ] #2 Switchbard owns all repository task-domain records in one central database; linked worktrees and supported clients see the same committed changes without a Git commit or PR.
- [ ] #3 Each repo can explicitly export and import one deterministic, versioned exchange file for collaboration through Git; normal edits do not modify repo files.
- [ ] #4 Import preserves newer local work, previews conflicts, handles replay and explicit deletions safely, and applies atomically; unknown bases or invalid scope cannot silently overwrite records.
- [ ] #5 Migration preserves every task lifecycle, custom content, hierarchy, goals, rank, identity and divergent worktree source; backup and recovery preserve post-cutover updates.
- [ ] #6 GUI, TUI, sb, queue, dispatch and refine use the same central mutation boundary; unavailable storage cannot fall back to writing stale backlog files.
- [ ] #7 Schema remains highly flexible: custom fields, nested values and custom sections can be added without SQL schema migration; known edits preserve unknown content; compatible older clients losslessly round-trip unsupported record content; typed projections never discard complete records. Prove through edit, reopen, exchange and projection-rebuild journeys.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Reviewed implementation contract: docs/decisions/switchbard-owned-storage/plan.md on branch codex/task-147-storage-contract, commit 4a50c90. Worktree: /Users/bpc/Dev/.worktrees/switchbard-task-147-storage-contract. Sequence: lossless identities/transactions; central task-domain commands; migration preview; all frontend/dispatch/refine consumers; deterministic optional exchange; recovery and real native evidence. No production implementation or migration has occurred.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
2026-09-08: Owner selected TASK-147 next. Beginning storage-authority discovery and decision work. Current task wording leaves the all-data boundary and repo-summary contract unresolved. Preserve existing records and production persistence during discovery; implementation acceptance must be refined before storage cutover.

Released unfinished by session codex-ta: Discovery complete, implementation remains unstarted. Awaiting owner clarification of all-data scope and repo-summary purpose. Evidence committed as b85bd2f on codex/task-147-storage-contract in /Users/bpc/Dev/.worktrees/switchbard-task-147-storage-contract/docs/decisions/switchbard-owned-storage/. Source audit and formatting passed; no migration or behavioral tests run. Resume original storage decision work after scope answer; reporter acceptance remains unchecked.

Owner clarification 2026-09-08: Switchbard owns and updates one central database for all repositories. Local worktrees share current data without PRs. Each repo may optionally carry one file committed/PR-ed for collaboration and sync. The file is an exchange mechanism, not the live authority for local worktree updates. Continue contract and migration planning around this clarified outcome.

Clarified scope supersedes the initial summary-only interpretation. Proposed engineering details are in docs/decisions/switchbard-owned-storage on codex/task-147-storage-contract. SQLite and .switchbard/tasks.json are proposed defaults; central local authority and optional Git exchange are owner-stated requirements.

Released unfinished by session codex-ta: Decision contract saved at 4a50c90 on codex/task-147-storage-contract. Independent findings closed; structural validator and plan lint pass, synthetic34/34, wire30/30, verifier harness11/11. Product acceptance remains34/34 RED: storage CLI missing. Implementation and reporter criteria remain unchecked; first persistence schema/migration boundary follows review of proposed SQLite plus explicit single-file JSON exchange details. No production data or shared app changed. Resume from reviewed plan in preserved worktree.

Fresh independent second opinion (2026-09-08): revise before implementation. Central SQLite authority supported. Reopened findings: skipped-export ancestry forces unchanged peers into reconciliation; bootstrap/edit/first-export model emits null base; aggregate goal/rank record granularity undefined; base64 plus full base/current harms PR review; exact peer journeys and revision-bound mutation evidence missing. Report and two reproduced synthetic probes committed as a095f8e on codex/task-147-storage-contract at /Users/bpc/Dev/.worktrees/switchbard-task-147-storage-contract/docs/decisions/switchbard-owned-storage/second-opinion.md. README readiness corrected; normative contract unchanged pending revision. No implementation, migration, push, or PR. Task remains incomplete.

Released unfinished by session codex-ta: Requested second opinion complete; revise-first report and reproducible probes preserved in commit a095f8e on codex/task-147-storage-contract. Implementation remains incomplete; contract revision is next.

Owner clarification 2026-09-08: preserve today’s highly flexible schema. Added governing schema-flexibility.md in the isolated TASK-147 contract package, with small stable envelope, extensible live content, unknown-field/kind/version preservation, collision handling and six required acceptance journeys extending MUST-004/017. Contract and wire entrypoints link the requirement. Schema/model/verifier revision and second-opinion findings remain open; no production implementation.

Released unfinished by session codex-ta: Schema flexibility planning amendment complete and committed as 7e1265f on codex/task-147-storage-contract. Six acceptance journeys and task AC added. Full contract/schema/verifier revision and implementation remain open.

Released unfinished by session codex-ta: Paused for user decision on managed review finding storage-migration-no-writer-quiescence and unresolved semantic reconciliation choices. Implementation preserved at3bbda583, pipeline01M21XJB3MRP2WB3CJ3V8C9AKX retains custody. Seven-repository rehearsal passed; no live activation/install/push/PR. Resume details: ~/.switchbard/migration-reviews/task147-20260908/current-handoff.md. Runaway yes cleanup complete.

Migration status: seven-repository disposable shadow rehearsal preserved task/project/initiative/goal views; 36 phases rehearsed, six held, 150 records, originals unchanged. No live authority activation or installation. Current no-mistakes run 01M21XJB3MRP2WB3CJ3V8C9AKX has passed focused storage and CLI journeys at 34d5ec1d; full preflight at 6c484a49 found 32 legacy write-test failures following writer-lock changes, now being fixed within pipeline custody. Independent public refine_task reproduction confirmed custom fields/sections retained centrally but omitted from actual model context when original Markdown is removed; evidence in private migration review custom-context-before/manifest.json. That consumer defect is also assigned to pipeline fix. Native writer/migration interleaving proof and final validation remain; no completion claim.

Owner changed rollout direction: begin live lower-impact migrations now without waiting for speculative concurrency test completion. Preserved all pipeline commits via guarded recovery at 6c484a49; validation ended with full preflight still red (32 standalone legacy write tests), not claimed green. Installed database-capable sb, sbt and signed Switchbard app with original binaries/bundle backed up privately under ~/.switchbard/migration-reviews/task147-20260909-live/installed-before. First LIVE cutover completed: CambridgeKitchens initiative 1 + projects 11, exact ordinary view parity, retained sources unchanged, default ~/.switchbard/switchbard.sqlite3 authoritative for those kinds. Continuing initiative/project phases across other rehearsed repos. Task records remain legacy pending concrete Refine custom-context fix and semantic conflict decisions.

LIVE update: all eligible lower-impact planning phases finished. 33 verified live kind cutovers across seven repositories, 76 central records. Initiatives/projects/goals centrally authoritative in all seven; configuration in six (Matterline held); ranking in six (Switchbard held). Every phase ordinary task/project/initiative/goal views identical and retained sources unchanged. SQLite quick_check ok and after-planning.sqlite3 backup fingerprint verified. Evidence ~/.switchbard/migration-reviews/task147-20260909-live/live-summary.json. The 32 concrete standalone write failures are now fixed in commit8b5d4bb, all35focused tests passing, pending refreshed installed build. Refine/dispatch custom-context repair is in progress; task authority remains on files until that concrete consumer issue is addressed.
<!-- SECTION:NOTES:END -->
