# Abstraction implementation mission

Objective: prioritize and implement TASK-144 and TASK-162 through TASK-167, exercise the actual sbt TUI throughout, and document encountered defects and feature requests through the native tracker.

## Ordered outcomes

1. TASK-162: shared column capabilities, entity adapters, deterministic sorting.
2. TASK-163: shared field parsing and matching through those adapters.
3. TASK-164: semantic paint precedence separated from terminal colors.
4. TASK-167: authoritative filing and merge dates in existing paint controls.
5. TASK-165: bounded presentation contracts for Tasks and Pull Requests.
6. TASK-166: explicit feature-scoped view capabilities and compatible persistence.
7. TASK-144: one keyboard action catalog for configuration, availability and help.

All retain medium priority from their recorded user/developer impact. Ordering follows dependencies; independent presentation and shortcut work may overlap. Actual primary tracker rank was updated with sb. Implementation starts at origin/main f5c5f61c, which includes the existing PR parity and picker consolidation; the user checkout remains on its original branch with its existing changes preserved.

## Acceptance and authority

Each issue's acceptance criteria remain authoritative. No human confirmation is inferred for TASK-144. Root owns integration, tracker closeout, verification, installation coordination and final claims. Teammates own disjoint source leases. No GitHub merge, PR write, or remote publication is required by this request. Runtime sbt interaction and the documented local install loop are in scope; shared GUI restart is excluded.

## State and stress evidence plan

| Dimension | Required evidence |
| --- | --- |
| Tasks/PR default and active | Real key events, disk-backed tasks, rendered cells and native PTY session |
| Empty and zero matches | Filter and list E2E journeys |
| Loading, stale, failure, retry | Existing PR observation journeys plus list contract coverage |
| Dirty drafts and save/restart | Existing task draft tests and slot/restart E2E journeys |
| Missing/invalid field and date | Matching and date paint E2E journeys |
| Long labels, multi-value, duplicates | Column, filter and picker rendered assertions |
| Narrow/current/wide and short viewport | 42/80/140 column terminal buffers, bounded picker/list navigation |
| Keyboard focus, repeated actions, page switches | Real-key journeys, independent Tasks/PR state |
| Large lists and bounded rendering | Offscreen/scroll/selection tests at documented N |
| Pointer/touch/browser zoom | N/A: terminal keyboard surface; terminal dimensions cover resizing |
| Remote mutation permissions | N/A: abstraction and read-only PR paint do not add remote actions |
| GUI render performance | N/A unless GUI render paths change; existing GUI primitives are inventoried read-only |

Evidence and remaining gaps are updated here and in the task-specific evidence documents. A green test or implemented slice does not close the seven-task objective.

## Initial observations

The installed sbt launched successfully in an agent-owned PTY at 80x24 and rendered Abstraction in the requested rank order with TASK-162's live claim. Existing primary changes were recorded outside the repository under /tmp/switchbard-abstractions-evidence. Shared target provenance risk is already tracked as TASK-179; this mission uses an isolated Cargo target for authoritative verification.
