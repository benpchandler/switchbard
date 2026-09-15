# Complete task details and confirmed cancellation

## Objective and authorization

The owner authorized the concrete detail-pane and `t c c` cancellation changes on 2026-09-15. Show every field before deciding which to deemphasize, allow sections to fold, and require confirmation to cancel a task. Dropping acceptance criteria must not require a reason. The suggested future planning vocabulary is Considering (not yet selected for work) / Planned (selected and ordered in the priority list); no planning-state, progress-percentage, completion-policy or Git-association migration is part of this slice.

The original checkout was clean at `da94a4ec`. Implementation is isolated on `feat/tui-detail-sections-cancel`, branched from refreshed `origin/main`. Cambridge task records are evidence only and must not be mutated for testing.

## Intended behavior

All currently modeled task fields are visible in named sections, including empty values, custom fields, implementation notes, plan, final summary, Definition of Done, references, ownership, dates, relationships and storage metadata. Sections start expanded, with no field-selection preferences. Existing editable fields keep their native write paths; read-only fields are explicitly readable. Focus with Enter twice, then use `z` for the current section, `Z` to collapse all, `A` to expand all, or click a section header. Collapsed headings remain keyboard navigable. Collapsing presentation must not mutate task data or discard input.

From the main Tasks list, `t c` opens a confirmation identifying the selected task. Another `c` confirms, while Escape or the default Keep task choice leaves it unchanged. Cancellation uses the existing archive lifecycle, preserves its contents, affects only the named task and explicitly discloses that children remain. It never marks dependencies satisfied or stops agents. Done tasks retain the existing refusal to archive.

The confirmation is bound to the observed task identity and revision/content. Missing, changed or reparented records require a fresh confirmation. No reason is required. Confirming is disabled until the complete confirmation fits on screen. No schema change is needed.

## Outcome ledger

| Outcome | Owner | Evidence / status |
|---|---|---|
| Complete collapsible pane | Detail worker | 30 editing, 9 pane and 4 section E2Es pass; historical children included |
| Confirmed cancellation | Cancellation worker | 7 cancellation E2Es and central revision/identity test pass |
| Integration, documentation, independent review | Root | Independent reviewer found no material remaining findings; 13 targeted E2Es passed |
| Scoped formatting, clippy and TUI/core tests | Root | Final TUI 374 passed / 22 ignored; core 733 passed / 2 ignored; formatting and all-targets clippy passed |
| Native rendered evidence and installation safety | Root | Real terminal buffers inspected and retained; installed `ed49358` lineage preserved |

## State and stress matrix

| Dimension | Required evidence |
|---|---|
| Default / empty / populated | Every section starts expanded; unset values and zero criteria remain visible; custom fields and long notes are readable |
| Folding / navigation | Header keyboard and pointer toggles, collapse all, expand all, cursor/scroll repair and selection preservation |
| Editing / dirty | Existing field edits and AC toggles persist; folding cannot discard active input; draft conflicts preserve input |
| Read-only / historical | Metadata remains readable; fields cannot accidentally mutate completed, archived or otherwise read-only records |
| Cancel / abort / success | `t c` does not write; `t c c` archives exactly one task; Escape/default Keep task do not write; success reloads selection |
| Changed / missing / conflict | Confirmation cannot target a replacement record or archive newer edits; errors preserve the original data |
| Related work | Parent children remain; open dependents remain blocked by canceled work; live claims/processes are not stopped |
| Small / wide / Unicode | 25x8, 32x8, 40x8, 60x12, 80x24, 100x20, 180x50 and 180x100; long titles/URLs, multiline content, non-ASCII; confirmation must fit before mutation |
| Repeated / rapid / resize | Repeat events do not confirm; stale frame/resize cannot bypass visible confirmation; duplicate confirmation is harmless |
| Storage | Legacy file fixtures plus central-store identity/revision tests; no live database writes |
| Offline / permissions | No network needed; local write failures are reported; remote-source refresh N/A |

Tests exercise real key events, real temporary backlog records and rendered Ratatui buffers. Visual evidence is separate from owner approval. Verification results and any unresolved gaps are recorded below before closeout.

## Verification

The first full TUI run found three old pane tests whose expected bottom/empty layout changed when additional fields became visible. They were updated to assert the new layout while preserving navigation checks; the nine pane tests then passed. The initial core library suite passed 694 tests (2 ignored), and core/TUI clippy passed.

Independent review identified an existing central-storage editing failure: the pane read retained Markdown instead of authoritative SQLite. A real imported-task fixture with the legacy file removed reproduced it. This slice also repairs authoritative draft reads and guards acceptance toggles against changed revisions. Review additionally caught historical children omitted from the new relation display and merge-specific wording in the shared narrow-confirmation fallback; both are covered by focused fixes.

Real Ratatui TestBackend screens were inspected at full, collapsed and narrow sizes, plus the cancellation confirmation. This is renderer evidence, not owner visual approval or proof of an installed build. Final core and TUI suites passed; guarded installation is assessed separately below.


### Evidence and limits

Actual terminal buffer captures are retained under [evidence/tui-detail-sections-cancel](evidence/tui-detail-sections-cancel). They show collapsed details, narrow 60x12 and 32x8 details, and a 100x20 cancellation confirmation. The all-field fixture is reproduced by `tests/detail_sections.rs`; the SQLite editing fixture is `tests/detail_central.rs`; cancellation is `tests/cancel.rs`. Captures are incidental verification evidence, not a separate design approval request.

Keyboard/pointer, dirty/stale, historical children, small-container clipping, absent legacy files and revision conflicts have automated coverage. Remote loading/offline, browser zoom/touch and remote permission roles are not applicable to these local terminal actions. No new asynchronous saving surface is introduced. Long text and small windows are covered; no new large-project performance benchmark or injected disk-full test was run. Direct non-cooperating filesystem writes cannot obey the repository fence; the native file writer retains its existing atomic-write behavior. New multiline content and metadata remain read-only.

The independent review found no material remaining issue after fixes. Central stale-checkbox refusal is directly covered; a dedicated central stale-field-patch E2E is not present. Existing field-conflict tests and the transactional revision guard provide narrower coverage.

### Final checks

- `mise exec -- cargo test -p switchbard-tui`: passed, 374 tests; 22 existing opt-in/ignored checks were not run.
- `mise exec -- cargo test -p switchbard-core`: passed, 733 tests; 2 ignored.
- `mise exec -- cargo clippy -p switchbard-tui -p switchbard-core --all-targets`: passed, warnings denied.
- `mise exec -- cargo fmt -p switchbard-tui -p switchbard-core -- --check` and `git diff --check`: passed.
- Independent review: no remaining material findings.
- Primary checkout remains clean. No Cambridge Kitchens task records were modified.

The guarded installer dry run (`bash scripts/install-switchbard.sh --dry-run --branch --hold sbt`) refused because this branch would drop 17 commits from installed `ed49358`. No binary was replaced and no force override was used. Code commit: `c725b99`. No push, PR or merge is part of this slice.

## Installed-build ancestry reconciliation

The owner requested resolution of the 17-commit install refusal on 2026-09-15. Both checkouts were clean before work. The installed TUI is `ed49358e` from `feat/tui-history-preview`; the feature branch starts from main and retains the validated detail/cancel changes.

The 17 absent commit identities are old feature/integration ancestry, not 17 missing product changes. PR #164 (`3bb50f6e`) explicitly incorporates the older wrapping, detail scroll, filter badge, resume and history work; PR #165 (`1be8678d`) incorporates the miniature history cards. PR #170 and `dd85a704` reconcile the read/edit detail interactions with current main and remove the obsolete `app/detail.rs`. History picker/title/projection sources match the installed build; remaining preview differences adapt to current task-row return values and the added Agents page.

A normal merge of the installed commit was inspected. Its conflicts attempted to restore superseded detail structures, older help layout and older startup behavior. Resolving those to the already-reconciled feature implementation left the complete source tree byte-identical to `dcbcb9f` before this evidence update. The resulting merge records the installed ancestor without bypassing the guard or removing newer main features.

Acceptance: installed commit must be an ancestor, ordinary installer must pass without force, new `sbt build-id` must name this feature build with `dirty=false`, and unrelated primary checkout changes must remain untouched. No remote push or PR merge is authorized by this local installation repair.

Verification: all install-guard regression cases passed; 15 history, history-recognition, preview and installed-compatibility tests passed. Prior complete source validation remains applicable because this reconciliation changes no source code. Independent audit and actual installation results follow.

Independent audit confirmed no missing installed functionality. Reconciliation merge `94df3d04` has installed `ed49358e` as its second parent and changes no product source relative to `dcbcb9f`. Ordinary guarded installation succeeded without force; `sbt build-id` reports `94df3d04`, `feat/tui-detail-sections-cancel`, `dirty=false`. Receipt outcome is `installed` with `dropped: []`. The standard feature hold expires at 2026-09-15 22:50:11 UTC (18:50:11 America/New_York); automatic main installation may resume after that. Both worktrees were clean after installation. This is a local feature install, not a remote merge.
