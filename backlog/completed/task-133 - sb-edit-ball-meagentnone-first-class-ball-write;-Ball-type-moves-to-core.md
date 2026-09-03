---
id: TASK-133
title: 'sb edit --ball me|agent|none: first-class ball write; Ball type moves to core'
status: Done
assignee: []
created_date: '2026-09-02 21:51'
updated_date: '2026-09-02 21:55'
labels:
  - cli
  - tasks
  - tui
dependencies: []
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Impact: the ball (who acts next: me / agent) is a TUI-only concept (tui/ball.rs) written as raw labels ball:me / ball:agent; agents and scripts driving sb had to reverse-engineer the label names from the sbt binary (observed 2026-09-02 in the CambridgeKitchens backlog). One authority for the label vocabulary belongs in switchbard-core beside the other write ops. Evidence: crates/switchbard-tui/src/ball.rs; app/mod.rs pass_ball() writes two set_backlog_label calls.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 switchbard_core::Ball (of/next/text/label + ball:me/ball:agent constants) is the single authority; the TUI re-exports it
- [x] #2 switchbard_core::set_backlog_ball(root, id, Option<Ball>) clears the other ball label and sets the requested one in surgical writes; byte no-op when unchanged
- [x] #3 sb edit <ID> --ball me|agent|none writes through it; prints Edited/no changes
- [x] #4 sbt pass_ball uses set_backlog_ball
- [x] #5 unit tests cover set/switch/drop
<!-- AC:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
switchbard_core::backlog::ball is the authority (Ball::of/next/text/label/parse, BALL_ME_LABEL/BALL_AGENT_LABEL); set_backlog_ball() in mutations; sb edit --ball me|agent|none; sbt pass_ball now one call. Tests: ball.rs (2), mutations.rs (1). Gates: mise run fmt/clippy clean, 505 core tests pass.
<!-- SECTION:FINAL_SUMMARY:END -->
