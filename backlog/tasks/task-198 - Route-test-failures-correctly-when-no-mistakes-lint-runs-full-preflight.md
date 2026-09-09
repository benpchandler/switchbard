---
id: TASK-198
title: Route test failures correctly when no-mistakes lint runs full preflight
status: To Do
assignee: []
created_date: '2026-09-08 16:43'
updated_date: '2026-09-08 17:14'
labels:
  - bug
  - tooling
dependencies: []
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Impact: maintainers cannot reliably complete delivery when the configured lint step runs full preflight: its fix-agent prompt forbids behavioral validation, and observed agents corrected only formatting or reported clippy clean while known source/test defects remained. Evidence: parent-picker run 01M21ANWNJ2F7XN0BKRRD7ARZV and Inbox run 01M21AD9CE4NSZE6P3GV964XSX on no-mistakes v1.60.2, installed commit eb4e37988b6e1bdb31f192ea0b1a9d6478495e43. Inbox agent99335 performed rustfmt only despite explicit all-target source repairs. Read-only SQLite and exact-source audit confirmed added findings and user instructions WERE passed to that lint fixer; this is agent adherence and phase-contract failure, not dropped AXI arguments. Source internal/pipeline/steps/lint.go appends finding JSON while restricting the phase to lint/static analysis; Switchbard commands.lint is mise run preflight. No supported mid-run phase rewind or alternate patch executor exists. Discovered during TASK-193 and authorized parent-picker merge.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A failing test in the configured preflight is surfaced with its test name, file and actionable failure details.
- [ ] #2 A requested fix corrects the known test failure or reports a concrete blocker; passing fmt/clippy alone cannot count as its repair.
- [ ] #3 Added-finding guidance is delivered to the fix agent and a complete preflight is required before push.
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Further live evidence invalidates the assumed add-finding workaround: Inbox run01M21AD9CE4NSZE6P3GV964XSX received detailed source/fixture fixes in --add-finding description and --findings lint-1 --instructions, but lint agent99335 performed formatting only. Current preflight still fails E0063 missing inbox_page and E0425 task_row. Existing description records an attempted workaround, not proven supported routing. Exact installed prompt propagation is under investigation; do not rely on added findings reaching the lint fixer until verified.

Exact source and database audit settled routing: previous assertion that instructions may be silently dropped was incorrect. They reached the constructed prompt. Latest repair is anchored to actual E0063/E0425 all-target Clippy/compiler failures, a materially different required static-analysis repair; source/fixture behavior corrections remain explicit.
<!-- SECTION:NOTES:END -->
