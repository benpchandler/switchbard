# Persistent asynchronous decisions

Owner request: 2026-09-16. Tracking: TASK-113; integration with the in-progress TASK-143 Inbox handoff work. Status: planned feature contract, not an implemented capability.

## Outcome

Switchbard is the companion app where agent questions remain available with their context while the user continues working asynchronously. Agents submit structured questions through the CLI, the user responds in a persistent Decisions inbox, and Switchbard records and delivers the exact response. The agent stays focused on its task, works on independent steps, and waits only at the affected decision boundary. Closing the app, exiting the agent, changing worktrees, or ignoring a notification never loses a question or implies an answer.

Every question automatically includes both “I don’t care - you decide” and “I don’t understand this - explain it.” These are system options; agents cannot omit, rename, or replace them. No answer is preselected or automatically submitted.

## User journey

An inbox entry shows a plain-language question, why it matters to the user's goal, the affected task and repository, the asking agent, and whether it blocks a specific next step. Opening it reveals retained context, links to relevant evidence, named options and consequences, an optional agent recommendation with its reason, and the permitted decision scope. Context must be understandable without opening the original terminal transcript. References supplement the retained explanation rather than replace it.

The user can select a supplied option, write an answer, delegate the choice, or request an explanation. An optional note can accompany every response. Drafts survive navigation and restart. Submission explicitly saves the response; a delivery indicator separately shows whether the agent has acknowledged it. Pending, clarification requested, answered but undelivered, delivered, withdrawn, and historical records remain distinguishable and searchable. The history includes the original question, every revision, every explanation and response, who submitted each event, and when.

“I don’t care” means explicit delegation for this question's stated scope. The agent records its resulting choice and reason back into the same thread. This is not permission to expand scope or bypass a separate destructive, publication, financial, or other explicit approval gate. If the delegated choice still encounters such a gate, the gate remains held and the UI explains why.

“I don’t understand this” records a clarification request and delivers it to the asking agent. The agent explains the issue in plain language, connects it to the user's goal, and reduces unnecessary jargon. The decision remains unanswered until the user chooses or delegates. A request for explanation is never approval of the recommendation.

The opt-outs are always available. Silence, age, dismissal of a notification, hiding a record, task completion, or ending a session never becomes a response. Users need not manufacture a technical opinion to clear an inbox item.

## Authority and records

Proposed implementation: a core-owned decision aggregate in Switchbard's central SQLite database, available across worktrees and frontends. Do not introduce a second question queue or a Markdown-only authority. The GUI, TUI, CLI, hooks, and supervisors call the same typed core commands. The precise schema and compatibility adapter need validation before migration.

A decision has a stable ID, repository identity, optional linked task and originating run/session, agent-provided idempotency key, current question revision, lifecycle, creation/update timestamps, and an append-only event history. Each question revision retains its title, question, motivation, context, evidence references, options with stable IDs and consequences, optional recommendation, decision scope, and the exact dependent step. Retain content if a task, worktree, session, or evidence path disappears; mark the reference unavailable.

Responses reference the exact decision ID and question revision and record response kind (option, free text, delegation, clarification), selected option when applicable, full user text, actor identity, timestamp, and a unique response/event ID. Question revisions and responses are immutable history. Amendments append a new event; they do not overwrite earlier user intent. Clarification appends an explanation, with a new question revision if options or scope change. A superseding answer is a new explicit human response; it cannot retroactively undo an action already taken.

Core enforces lifecycle transitions, repository/session routing, valid option IDs, stale-revision rejection, system options, input bounds, and idempotency. Database transactions atomically commit an event and its delivery record. Retrying the same submission returns the existing result; using its idempotency key with different content refuses. Competing answers to the same open revision conflict instead of silently choosing the last writer. Retry after an unknown save outcome reads back by request identity before attempting a new write.

An answer is durable before delivery. Readers receive full payloads and explicitly acknowledge the event after successful receipt and local recording. Polling or printing never consumes an answer. Redelivery is expected until acknowledgement; agents deduplicate by event ID before taking actions. Acknowledgement proves receipt, not implementation or task completion. If the original agent is unavailable, the response remains visible and undelivered with an explicit reconnect or handoff action. Do not silently dispatch a replacement or wake an unrelated session.

## CLI and hooks

Proposed `sb decision` contract, to be finalized against native command conventions:

| Command | Meaning |
| --- | --- |
| `ask --file <json> --request-key <key>` | Submit a complete question, preferably from a file or stdin; return ID, revision, status and next step |
| `list`, `view <id>` | Read questions and complete history, with repository/task/state filters and machine-readable output |
| `respond <id> --revision <n> --kind <kind> ...` | Record an explicit user response; validate actor provenance and the displayed revision |
| `explain <id> --revision <n> --file <json>` | Append the agent's explanation without manufacturing a user answer |
| `events --session <id> --after <cursor>` | Read addressed responses and clarification requests without consuming them |
| `ack <event-id> --session <id>` | Idempotently acknowledge delivery to the intended consumer |
| `withdraw <id> --revision <n> --reason <text>` | Retain a no-longer-needed question with a reason; never label it answered |
| `hook <event>` | Adapter for supported runtime hook payloads; submit or retrieve using the same core boundary |

Human responses must have explicit human provenance; an agent cannot turn its recommendation into a human answer through a hook. Local actor labels are provenance, not a claimed security boundary against arbitrary local processes. The implementation must define the trusted human-input paths and reject accidental self-answering through agent-facing adapters.

Machine output contains the full structured payload only on stdout. Errors and progress use stderr. Every status supplies a `next_step`. Bounded foreground waiting may be provided, with timeout/interruption distinguished from an answer; it must not consume events. No hook rewrites runtime settings automatically. Provide opt-in configuration examples for supported Claude and Codex integration points, verify the payloads those runtimes actually expose, and document unsupported hooks. Do not rely on scraping arbitrary terminal prose as the authoritative question.

At task/session start and subsequent supported boundaries, agents retrieve outstanding events for their exact identity. A question-submission method records the question and its dependent step; it permits independent work to continue. A stop-time adapter must preserve pending questions and handoff context even if the process ends. Hook failure preserves payloads, reports recovery, and cannot silently select an option or block all unrelated work.

## Repository grounding and integration

Verified on 2026-09-16:

- `crates/switchbard-core/src/storage/mod.rs` owns the central SQLite store, versioned documents, repository identity and change sequence. Existing recognized document kinds do not include decisions.
- `crates/switchbard-gui/src/ui/places/command.rs:50` explicitly documents the evidence-only support card and absent structured question store. Its Respond action currently links to the task.
- `docs/tui-inbox-badges.md` defines the shipped Inbox destination as a placeholder, with owner action counts deferred.
- TASK-113 already tracks the missing support-request store. This request extends that task rather than duplicating it; general SITREP storage is outside this decision feature.
- TASK-143 and the separate `feat/tui-bug-dispatch` worktree contain a bug-specific durable Inbox and exact Codex thread resume. Its `switchbard-core/src/bug_run/` and TUI Inbox modules are integration candidates, not established main-branch generic decision authority. Preserve that work and settle its delivery lineage before overlapping implementation.
- `mission_sidecar_protocol.rs` already exposes xplan pending/resume decision operations. xplan remains the sole mission writer. Mission integration must route through its strict versioned protocol and represent delegated/clarification responses honestly; do not independently resolve an xplan-owned mission in Switchbard's database.

Use one inbox with decision entries and existing review handoffs. Command support cards and task details link into the same records. The native companion GUI is required; the CLI and TUI are additional consumers. Do not call this feature delivered based on a headless store or a TUI-only slice.

## State and stress matrix

All evidence below is a future implementation gate. No GUI states have been rendered or approved for this feature.

| State or stress | Required behavior | Evidence gate |
| --- | --- | --- |
| Empty, loading, active, historical | Honest empty/loading state; pending and history distinguishable; count comes from core attention facts | Native GUI/TUI fixtures and domain assertions |
| Dirty draft, navigation, restart, upgrade | Preserve full unsent user input; no discard from Escape or reload | Persistence and key-driven E2E |
| Saving, success, error, retry, unknown outcome | Save status distinct from delivery; readback before retry; draft retained | Database fault and interruption tests |
| Stale question, simultaneous answers, changed options | Reject obsolete revision/invalid choice; retain draft and history; offer refreshed context | Two-client transaction tests and native conflict fixture |
| Delegation, clarification, repeated clarification | Both opt-outs always present; scoped agent choice logged; explanation never resolves question | Domain tests and real agent/user round trip |
| Duplicate submissions, replay, ack crash | One question per request identity; events retained until ack; no repeated action from replay | CLI/database fault-injection integration |
| Agent exits, session resumes, deleted task/worktree | Questions and answers survive; orphan references explicit; route to original identity or user-authorized handoff | Restart/resume and orphan tests |
| Offline, unavailable database, read-only permissions | Explicit failure/recovery; no fallback to a divergent file authority; no inferred zero count | Permission/corruption/offline tests |
| Zero, one, many, long context/options, Unicode, unbroken strings | Paginated bounded reads, wrapping and searchable full content; no silent payload truncation | Large deterministic fixtures and parser bounds |
| Narrow/wide, short window, enlarged native text, keyboard/pointer | Context/options and opt-outs reachable; predictable focus; no clipping | Real native renders, accessibility and layout review |
| Slow save/poll, rapid submit, navigate while saving | Responsive render path; preserve pending operation; no duplicate response | Latency E2E and GUI p95 performance smoke |
| Removed access, unsupported content/client version | Retain opaque content, display recovery; refuse unsafe edits | Compatibility and read-only fixture tests |
| Touch/browser zoom | N/A: native desktop and terminal; native scaling is covered above | N/A |

## Ordered implementation and acceptance

1. Reconcile the generic aggregate with TASK-143's existing message, delivery and exact-thread-resume behavior, and xplan's distinct write authority. Define concrete payload limits, lifecycle transitions, actor provenance and compatibility before migrating. Add valid, stale, replay, clarification and delegation fixtures.
2. Implement the core aggregate, transactional database persistence, history and reliable delivery first. Validate an isolated database through restart, replay, concurrent writes and partial failure. Preserve existing task and bug-run authority and data.
3. Implement `sb decision` JSON/help/error contracts and opt-in runtime adapters. Prove submit, poll, respond, explain and acknowledge against the same database used by the frontends, with the GUI closed.
4. Build native companion Inbox list/detail/response/history, draft retention, attention counts and Command/task links. Integrate the TUI and bug handoffs through the same authority. Render the matrix and run layout, keyboard, accessibility and performance checks.
5. Run a real paired agent/user task: agent submits, user continues other work, app and agent restart, user requests explanation, user responds or delegates, original agent receives and acknowledges, and the chosen action is recorded with provenance. Separately demonstrate that silence and clarification never authorize a held action.

Acceptance requires all five steps and their behavioral evidence, normal repository gates, schema upgrade/backup compatibility, and an owner review of the native workflow. A task checkbox, an API stub, a mock screenshot, a successful resume call, or a merged prerequisite is not proof of the complete feature. Rollout must preserve existing records on old-client refusal; reversing a binary upgrade must never drop new questions or responses. No schema migration, hook installation, or running-app restart is authorized by this planning artifact alone.
