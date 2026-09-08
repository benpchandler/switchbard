# In-app PR merge: objective and verification ledger

The owner requested an in-app PR merge option on 2026-09-08 as an addition to TASK-141. TASK-141.5 tracks this direct-merge slice of TASK-119. Prior browser opening, alerts, countdown, list controls and independent task state remain acceptance requirements.

Worktree `/Users/bpc/Dev/.worktrees/switchbard-tui-pr-merge`, branch `feat/tui-pr-merge`, was clean at eec92a0 before editing. It preserves all existing Task 141 commits. The earlier no-mistakes run 01M1YEPAFGEEH4DE72H3067KYG remains parked on `feat/finish-task-141`; neither its worktree nor custody is changed by this new additive request. Primary checkout ranking.yml and TASK-150 dirt remain untouched.

## Contract

- Configurable `m` on Pull Requests prepares a fresh merge preview off the input thread. Empty, historical, draft, blocked, unknown, unavailable and queue-required states explain why direct merge is unavailable.
- The confirmation uses the existing picker, names repository, PR, head revision, base and authenticated viewer, and lists only repository-enabled merge methods. Cancel is selected by default. A deliberate method shortcut confirms; typeahead cannot submit a merge.
- The core owns eligibility and exact identity. Submission refreshes head, base, viewer and readiness, sends an expected-head guard to GitHub, and never exposes an admin override or deletes branches. UI callers cannot fabricate a prepared eligible command.
- GitHub merge-state observations, not the list's aggregate checks, establish readiness. Normal merge operations remain available to administrators when ordinary readiness checks pass. GitHub has no atomic base/policy pin; only the head comparison is server-enforced, and no stronger atomic-policy guarantee is claimed.
- One merge at a time; browsing remains responsive. Outcome readback distinguishes confirmed, rejected, and unknown. A local receipt is persisted before dispatch and updated after readback. Preparation cancellation submits nothing; an ambiguous submission is not retried automatically.
- GitHub remains authoritative for PR state. Merging never marks a local task Done. Auto-merge, merge queue actions, review submission, CI reruns and branch updates are outside this slice.

## Coordination and proof

Parent owns integration, tracker, tests, docs, installation and delivery. merge_core owns core API/guards/receipts/tests; notifications owns TUI source integration. audit_141 independently investigated official GitHub behavior and will audit integration. No worker may spawn further agents or mutate a live PR during verification.

## State and stress matrix

| State | Verification |
| --- | --- |
| Empty / no selected PR / Tasks page | Real-key E2E; no worker or command dispatch. |
| Preparing / repeated m / cancel / page change | Real worker/read-only journey; cancellation cannot later reopen a confirmation. |
| Eligible / method choice / Cancel default / typeahead | Core state tests and live read-only confirmation/cancel journey. |
| Closed / merged / draft / conflict / queue / permission denied / unknown readiness | Core eligibility tests; historical and unavailable real GitHub reads. |
| Changed head / base / viewer / method or policy removed | Core execution guard tests prove no mutation call. |
| Submitting / duplicate action / page switch / quit / self-reexec | TUI flow guards; core receipt written before command; no blind repeat. |
| Confirmed / rejected / timeout / readback failure / receipt failure | Core behavioral tests with transport outcomes; real successful remote merge requires separately authorized execution. |
| Narrow / current / wide / short / long identity / Unicode | Ratatui real-key renders at 40x12, 80x24, 120x40, 180x50; full identity retained in bounded confirmation. |
| Zero/one/multiple methods | Core repo capabilities + confirmation choices. |
| Restart / interrupted operation | Durable core receipt survives process lifetime; no automatic resubmission. |
| Pointer / touch / browser zoom | N/A: keyboard terminal UI; terminal dimensions are the layout boundary. |
| Native screenshot / real remote merge | Explicit evidence gaps unless captured/executed; no claim of reporter approval. |

## Official semantics

GitHub CLI supports an expected-head guard and a separate explicit admin override: https://cli.github.com/manual/gh_pr_merge . The underlying merge mutation accepts `expectedHeadOid`: https://docs.github.com/en/graphql/reference/input-objects#mergepullrequestinput . Eligibility is re-read before using the normal merge mutation; no admin-bypass affordance is provided.

## Verification so far

Core worker ran seven scoped tests, including authenticated historical PR137 preparation returning Disabled. The parent then ran real-key historical rejection and leave-during-preparation journeys, both passing against GitHub. `mise run tui-install` passed formatting, clippy, the full TUI suite and release installation. Full workspace preflight and independent adversarial review are in progress. The positive eligible confirmation/cancel journey will run against the delivery PR when GitHub reports it ready; no production merge is part of verification.

## Local delivery gates

Full `mise run preflight` passed (workspace formatting, clippy, all-target tests, and developer-gate contracts). The final focused core run passed ten guard/receipt/readback/identity/method tests; the authenticated historical read-only case had already passed separately. Independent adversarial review found no source blocker. Its requested additional selection/method tests were added and passed. The terminal test suite also includes a live eligible-PR test for Cancel default, typeahead refusal, 80/120/180-column rendering and 40x12 confirmation refusal; execution of that final live test is pending the delivery PR.

Explicit remaining matrix gaps: actual remote merge, submit-time quit/reexec behavior under a real write, extreme Unicode/title stress, and native screenshot/reporter approval. These are not represented as completed live evidence. The receipt/core transport tests prove successful, rejected and unknown-result logic without changing GitHub.
