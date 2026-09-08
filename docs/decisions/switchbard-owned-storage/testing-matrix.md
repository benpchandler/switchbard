# Required evidence and state coverage

Owner clarification (2026-09-08): [schema flexibility](schema-flexibility.md) is a governing requirement. Use a stable envelope with extensible content, preserve unknown fields and kinds, and avoid schema migrations for custom fields. It extends MUST-004 and MUST-017. Earlier wire/schema/model details require revision where inconsistent; implementation readiness remains open.

Synthetic checks pressure-test decisions only. Product acceptance must execute the real core/CLI/UI seams; a green model is not implementation evidence.

| Required outcome | Product evidence required |
| --- | --- |
| Central authority for every task-domain record | Two repo scopes in one temporary product DB; task, hierarchy, rank and goal commands persist/reload without legacy files |
| Worktree and clone identity | Two linked worktrees see the same mutation; separate repo TASK-1 stays separate; explicit clone bind/repo move; fork not auto-bound |
| Concurrent writes and retry | Independent processes, expected revision conflict, same-command replay, mismatched replay, injected mid-transaction failure |
| Lossless migration | Unknown keys and custom body bytes, every lifecycle directory, hierarchy/goals/rank, divergent sibling and branch sources, changed-input abort |
| Optional one-file export | No filesystem changes on ordinary updates; byte-stable repeat export; one file round-trips all task-domain records; independently changed export target protected |
| Safe collaboration import | Empty-store bootstrap, known base, disjoint record merge, replay, stale preview, unknown base, same-record conflict, tombstone versus edit, omissions, complete graph validation |
| Bounded untrusted file | Unsupported schema, malformed JSON, conflict markers, wrong repo/digest, depth/count/byte limits, duplicate IDs, links and cycles reject with zero live effects |
| Supported consumers | GUI/TUI/CLI/queue/dispatch/refine use the same central writer; refine preserves raw bytes; task work survives execution worktree deletion |
| Failure and recovery | Unavailable/corrupt DB is explicit and cannot trigger Markdown fallback; consistent backup and restored post-cutover changes; import failure keeps legacy authority |
| User experience | Two running clients show committed change within two seconds while active; no PR; dirty editor conflict preserves draft; reporter confirms workflow |

## State and stress matrix

| Dimension | Applicable states and gate |
| --- | --- |
| Lifecycle | Empty/unregistered, legacy-only, migration preview, conflict, migrated, normal read/edit/save, saving, success, failure, retry, unknown outcome, stale preview, read-only recovery. Real CLI journeys plus GUI/TUI behavior tests and visual review of new messages. |
| Content and scale | Zero/one/many repositories and records; 100,000-record limit and one over; 64 MiB envelope and one over; 4 MiB payload and one over; depth 32 and one over; duplicate display IDs; raw multiline/Unicode/unbroken text. Parser boundaries, round-trip tests and bounded performance probes. |
| Container and input | Existing narrow/current/wide GUI; TUI 80x24 and current size; keyboard-only conflict/preview navigation; focus and scrolling; retained drafts. Real-container fixtures and layout assertions for any added UI. Touch N/A: no new touch workflow. |
| Transitions | Concurrent edits, repeated export/import, branch checkout, worktree deletion, repo move, app restart/refocus, independent clone bootstrap, interrupted apply, restore. Multi-process tests and real user journey. |
| Failures | Locked/unavailable/corrupt DB, malformed file, unknown base/version, wrong scope, export rename failure, stale digest, partial transaction failure, Git conflict markers. No partial writes or silent fallback. |
| Authority | Repo bind/fork/cross-scope rejection, explicit import apply, explicit conflict resolution. Network authentication/roles N/A: local database and user-mediated Git transport; no hosted service. |

No new UI states have been rendered in this decision pass. Those states are future acceptance gates, not approved designs. Performance evidence must use the actual supported app containers; a synthetic SQLite timing does not prove GUI responsiveness.
