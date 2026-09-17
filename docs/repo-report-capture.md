# Repository idea and bug capture (TASK-275)

`i` opens an idea draft and `b` opens a bug draft for the repository being viewed, on every page. The existing inline command editor supplies text entry, Enter to submit and Esc to cancel. The async report flow supplies Saving, duplicate prevention, result feedback and retry. The task menu's `t b` continues to assign the ball. Lua action names `repo_idea` and `repo_bug` allow key remapping.

A typed report scope and pinned target travel with a capture draft, submission and failed retry. Repository capture ignores the legacy `report_repo` redirect; colon `:bug` and `:idea` retain their existing tool-report behavior. Config reload cannot reroute a failed capture to another repository. Changing the report verb changes kind within the same draft scope; canceling discards the active draft scope.

Repository captures use the native configured initial status, medium priority and no assignee. Ideas are unassigned to a project. Bugs use an existing Bugs project when available, otherwise remain unassigned. Titles, labels and acceptance criteria describe the repository request without asserting an sbt defect. Per-action project, status, priority and other settings remain follow-up work.

## State and stress matrix

| State / transition | Evidence gate |
| --- | --- |
| Default / empty / one capture | Real-key i/b, empty intent rejection, successful native task |
| Every page / cancellation / keyboard focus | Tasks, PRs, Agents, Inbox; Esc leaves no task |
| Saving / queued / duplicates / navigation | Two-second real RepositoryLock, bounded input, one record |
| Failure / retry / redirect / config changes | Real storage failure, restored same-scope pinned target despite report_repo |
| Native status / Bugs present or absent | Custom status fixtures and project membership assertion |
| Unicode / long content / narrow-short / normal-wide | Real command draft, 40x8 and normal rendered result |
| Remapping / help / existing task ball | Lua custom keys, catalog help, t b native ball action |
| Stale / conflict / interruption | TASK-248 guards retained; submission retry does not infer task completion |
| Unknown result | Existing explicit unknown report state retained; no automatic retry |
| Read-only navigation / unavailable storage | Existing cached navigation with pending storage; genuine failure retains draft |
| Zero / many tasks / mixed kinds | Existing task-list bounds; captures independent of selected row |
| Pointer / touch / browser zoom | N/A to inline keyboard terminal capture |
| Role changes / remote access removed | N/A to local repository creation; filesystem denial is failure |
| Forced shutdown recovery | Explicit gap: no new durable crash recovery |

The implementation includes the editor and worker routing hooks. `ReportRoute` captures scope and the exact destination at draft opening; retry and queued drafts retain separate routes, so one save completing cannot erase or reroute another draft. Exact native task IDs and legacy numeric IDs both select their filed task.

Focused validation passed after integration of the final TASK-248 pending-storage guard: 26 tests across `repo_report_capture` (6), `ball` (5), `report` (7) and `shortcuts` (8), run serially against the isolated feature target. The report contention matrix exercises the legacy direct ball action through an explicit `B = 'ball'` remap while `b` retains repository capture semantics. Formatting and whitespace checks passed.

This proves the scoped behavior and rendered TestBackend journeys described above. Full delivery gates, PR/merge and installed-build proof remain pending; no owner visual approval or completed delivery is claimed. Forced-termination recovery remains outside this change.
