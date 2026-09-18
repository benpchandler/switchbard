# Bug dispatch and paired Inbox dogfood

Owner request: build TASK-143 for bugs specifically, and test the task -> agent -> Inbox -> PR flow as a human/agent pair. Clarification on 2026-09-16: file the bug then dispatch; Codex preferred.

Initial checkout: main at 1eda6e3e, clean. Implementation lease: feat/tui-bug-dispatch in /Users/bpc/Dev/.worktrees/switchbard-bug-dispatch, branched from fetched origin/main. TASK-143 edits use sb against canonical storage. No application restart or replacement yet.

## Required outcomes

1. A submitted :bug preserves title, screen and trail before dispatch starts. Ideas remain capture-only.
2. Codex runs against an isolated worktree, with durable identity and no duplicate execution.
3. Inbox presents questions and review requests with exact task/run association. Human drafts and sent replies survive restart. Replies resume the same Codex thread.
4. Failed launches, failed runs, unavailable dependencies, interruption and unknown outcomes remain actionable without losing the bug or reply.
5. A real bug is reproduced and fixed, reviewed by the owner through Inbox, and linked to a reviewable PR. Automated verification, owner acceptance, PR and merge are separate facts.

## Authority and boundaries

Implementation and a reviewable PR are authorized. Owner replies and approval cannot be fabricated. Merging is not yet requested. No broad dispatch-queue drain: TASK-113 is already queued and is outside this run. Core remains the sole durable mutation owner. Agent session state must not become a second task authority.

## Sequence and evidence

- Verified: report.rs creates a task with evidence; Inbox renders only a placeholder; existing dispatch runs Claude once and then opens a PR.
- Verified: local Codex exec and exec resume support JSON events, output schema and final message file. Official noninteractive documentation: https://developers.openai.com/codex/noninteractive/.
- Implemented: core-owned durable run/handoff contract, detached Codex supervisor, CLI entry and TUI Inbox with disjoint leases.
- Passed: state/stress tests and independent recovery/publication review. Pending: full delivery gate, live owner interaction and PR delivery.

## State and stress contract

| Dimension | Required cases | Evidence gate |
| --- | --- | --- |
| Lifecycle | empty, queued, launching, running, needs answer, review, publishing, PR open, failed, unknown, retry | Core transitions and real-key Inbox tests |
| Content | one/many bounded runs, long multiline prompts and replies, Unicode, repeated titles | Rendered 40x8, 80x24, 120x40; identity-based operations |
| Input | keyboard navigation, answer editing, explicit send, draft preservation, opening PR | Real-key tests and owner run |
| Concurrency | repeated send, two TUI readers, stale reply, concurrent launch | Transaction/lock and stale revision tests |
| Recovery | TUI exit, agent exit, missing executable, invalid response, interrupted publish | Durable reload tests; explicit unknown and recovery |
| Performance | slow agent/network does not block input, bounded records/output | Background worker tests and native interaction |
| N/A | browser zoom, touch | Native terminal surface |

No acceptance criterion is complete merely because implementation or a test exists.

## Verified development evidence (2026-09-16)

- Full pre-existing TUI suite passed in `/tmp/switchbard-bug-tui-suite.log`; subsequent targeted Inbox/capture suite passed in `/tmp/switchbard-bug-focused.log` after the confirmation and help updates.
- New real-key tests cover saved Unicode/multiline replies across restart, conflicting draft preservation, duplicate send, task-first capture, capture-only ideas, narrow/wide render, remapped publication key and confirmation that cannot submit while clipped.
- Core suite passes 26 store/process/Git tests and all-target core clippy. Includes concurrent claim, frozen report snapshot, resumed thread, missing executable, process interruption and child liveness, unsupported schema, symlink refusal, protected task content, pinned review and idempotent task PR references.
- Live Codex protocol smoke passed using authenticated local `codex exec` with read-only sandbox and no tools or file edits. Exact `thread.started` identity `01a0a89a-1221-7531-a83f-b70078fab522` was reused by `exec resume`; both returned the requested `awaiting_answer` schema. Logs: `/tmp/switchbard-codex-protocol-smoke/`. This is protocol evidence only, not a real bug fix or owner response.
- TUI all-target clippy passed in `/tmp/switchbard-bug-clippy.log` after correcting one useless-format warning.

Final integrated checks: 26 core bug-run tests, 26 targeted TUI tests and TUI all-target clippy pass after the last hardening changes. Independent review found no remaining blocker. Guarded startup now persists the process group before releasing execution; publication pins the GitHub push destination, tracks writers and reconciles interrupted push/create.

Remaining: pipeline review/PR/CI, native owner dogfood against a chosen real bug, owner acceptance. No default application replacement, live bug dispatch, human answer, PR or merge is claimed by the evidence above.
