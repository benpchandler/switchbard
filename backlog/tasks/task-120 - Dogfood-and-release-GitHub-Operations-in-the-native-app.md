---
id: TASK-120
title: Dogfood and release GitHub Operations in the native app
status: To Do
assignee: []
created_date: '2026-09-01 17:12'
labels:
  - github
  - ops
  - dogfood
  - verification
dependencies:
  - TASK-119
priority: high
project: GitHub Operations
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Impact: A green unit suite alone would not prove that GitHub Operations is usable, legible, safe under real account permissions, or responsive at repository scale.

Evidence: The acceptance contract requires native-app proof across real pull-request states, explicit failure semantics, and visual state coverage before the feature can be treated as usable.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A real non-production pull request proves observation, review, failed-check rerun, branch update, auto-merge or merge-queue behavior, and exact-head safeguards as applicable
- [ ] #2 Native macOS app evidence covers loading, empty, partial, stale, denied, conflict, offline, success, narrow-window, keyboard, and high-volume states
- [ ] #3 Background work keeps the UI responsive and bounded observations meet the documented latency and memory budgets
- [ ] #4 Automated tests, repository gates, and an independent adversarial review pass without unresolved high-severity findings
- [ ] #5 Visual Review records explicit human approval or unresolved feedback; zero annotations alone is not treated as approval
<!-- AC:END -->
