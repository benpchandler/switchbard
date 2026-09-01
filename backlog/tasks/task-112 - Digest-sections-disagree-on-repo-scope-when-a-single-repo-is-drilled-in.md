---
id: TASK-112
title: Digest sections disagree on repo scope when a single repo is drilled in
status: To Do
assignee: []
created_date: '2026-09-01 08:20'
labels:
  - gui
  - digest
dependencies: []
priority: low
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Goal cards narrow by backlog_view.selected_repo (via scoped_repos) while In flight / Needs a human use repo_scope only (audit evidence from PR #84's review, TASK-99 Digest place).

Impact: the landing page shows inconsistent scopes across its own sections after a user drills into one repo under Tasks - goal cards narrow to the drilled repo while In flight/Needs a human keep showing every repo in the sidebar's multi-select scope, so the three sections of one page tell different stories about what's happening.

Evidence: PR #84 review (TASK-99 Digest place); goal-card render path keyed on backlog_view.selected_repo/scoped_repos vs the in-flight/attention-feed render path keyed on repo_scope only.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 One scope authority (repo_scope vs selected_repo) applied consistently across Goal cards, In flight, and Needs a human sections
- [ ] #2 Test proving all three Digest sections agree on scope when a single repo is drilled into
<!-- AC:END -->
