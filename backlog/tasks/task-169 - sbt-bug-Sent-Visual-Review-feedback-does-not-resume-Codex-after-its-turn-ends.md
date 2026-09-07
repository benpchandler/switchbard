---
id: TASK-169
title: Sent review feedback does not resume Codex after its turn ends
status: To Do
assignee: []
created_date: '2026-09-07 13:04'
updated_date: '2026-09-07 13:05'
labels:
  - bug
  - agent-runtime
  - review-feedback
dependencies: []
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Impact: People reviewing agent-created visuals can submit feedback successfully yet receive no response after the Codex turn ends. They must return to chat to prompt retrieval, so review work silently stalls despite a healthy listener. Priority: medium, because feedback remains recoverable and a manual chat prompt unblocks it; no data loss was observed.

Observed in the Lucella login-layout review on 2026-09-07. The agent started visual-review poll as exec session 59359, then delivered its final response while that process remained alive. The reviewer submitted "Should this be centered on the page?" at 2026-09-07T16:58:56.036Z and ended the review. No agent response followed until the reviewer sent a chat message. Resuming session 59359 via write_stdin returned exit 0, status=ended, and the complete saved annotation. This proves capture/delivery to the process succeeded, while automatic agent resumption did not.

Evidence: target d7e43e94-8612-4e5a-9790-9e664ef831c4-ad65101f498f; annotation annotation-1b4fd442be434dad9d3463d6272c5957; owning review path /Users/bpc/Dev/.worktrees/budget-auth-transitions; canonical review store is outside the removed worktree. Local source /Users/bpc/Dev/visual-review/README.md:188-197 and src/cli/main.ts:60-64 explicitly warn that a backgrounded poll can finish without resuming the agent. Discovered in the Codex budget conversation after budget PR965 merged. Filed through sbt :bug; the automatically attached sbt screen is filing context, not a reproduction of the listener bug.

Reproduction: start a review and poll in a persistent exec session; let the agent end its turn; submit a comment with Send to agent or Send & End; observe that the comment is persisted and the poll completes but the owning agent does not respond without a new chat message.

Scope/ownership: filed in Switchbard at the owner's request as an agent-supervision integration defect. The observed failure is not established to originate in Switchbard itself. Trace the runtime/tool-completion wake boundary before choosing the fix. Options to evaluate explicitly: a supported durable runtime wake/resume adapter, or foreground-wait enforcement with a visible unsupported-runtime status. Merely starting a process must not count as verified live monitoring. Do not reopen a review ended by its reviewer.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Reproduce and record submitted feedback reaching the listener after the owning agent turn ends, distinguishing persistence, listener completion, and agent delivery.
- [ ] #2 On supported runtimes, sent feedback resumes the correct agent and is handled once without a new chat prompt; prove this end to end after turn completion.
- [ ] #3 When automatic resumption is unsupported, the UI and agent must not claim live monitoring; enforce or clearly expose the active-wait requirement.
- [ ] #4 Cover Send to agent, Send & End, interrupted listener, duplicate delivery, and worktree removal; preserve saved feedback and never reopen an ended review automatically.
- [ ] #5 Document the actual integration owner and selected wake or fallback mechanism, with regression tests at the tool-completion boundary.
<!-- AC:END -->
