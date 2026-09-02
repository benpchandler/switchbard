---
id: TASK-80
title: Make the Task Queue aware of GitHub delivery state
status: To Do
assignee: []
created_date: '2026-08-31 21:44'
updated_date: '2026-09-01 21:42'
labels:
  - github
  - task-queue
  - delivery
  - product
dependencies: []
priority: high
project: Task Queue
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Outcome

Switchbard presents one trustworthy task queue across local work and GitHub delivery work without creating two independently editable trackers. Switchbard remains the working plane; GitHub remains the delivery ledger and the authority for repository-controlled state.

## Authority boundary

- Switchbard owns objectives, priority, ordering, orchestration context, dependencies, assignments, leases, and outcome acceptance.
- GitHub owns issues when used for delivery, pull requests, reviews, required checks, merge queue state, commits, tags, releases, deployments, and repository automation.
- GitHub-derived facts are visibly sourced, freshness-aware, and never manually re-entered as authoritative Switchbard claims.
- Existing repo-local task files remain the Switchbard-owned working store; this project does not require a task-storage migration.

## Dogfood target

The Lucella Domain Migration GitHub project should appear in Switchbard with its open next work, including staging repair, Firebase/Google OAuth, backend identity, contact identities, transactional email, integration migration, and final production reconciliation.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Switchbard can present Switchbard-native and GitHub-backed work in one Task Queue without requiring duplicate task creation.
- [ ] #2 Each GitHub-backed item shows its source repository, issue, pull requests, checks, merge state, commits, releases, deployments, last refresh, and explicit Unknown states where GitHub cannot answer.
- [ ] #3 The ownership and conflict rules between Switchbard fields and GitHub-derived fields are documented and enforced by the write boundary.
- [ ] #4 Unauthenticated, insufficient-scope, rate-limited, missing, and stale GitHub responses degrade honestly and never produce false Done, merged, released, or deployed claims.
- [ ] #5 Lucella GitHub Project 3 can be used as a live dogfood proof that its next work appears in the intended priority order.
- [ ] #6 Existing repo-local task records remain readable and writable through Switchbard with no storage migration.
- [ ] #7 Core tests, UI state-and-stress coverage, and a rendered review prove empty, loading, partial, stale, error, mixed-source, and high-volume Task Queue states.
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Memory-hygiene lineage note (verified 2026-09-01): feature/task-queue-contract is a noncanonical legacy implementation at d58430d, 2 commits ahead and 141 commits behind current origin/main. It contains pre-approval implementation and evidence; the historical memory also named pipeline head ee4a592b, which is not available in the current local object store. Do not resume, merge, or cherry-pick that branch as product authority. Reconcile or retire it through this tracked Task Queue project against the completed TASK-80.2 canonical contract at docs/decisions/task-queue-authority-model/, current main, TASK-80.3 and TASK-80.4, these acceptance criteria, and current tests.
<!-- SECTION:NOTES:END -->
