---
id: TASK-80.1
title: Prove the Task Queue with the Lucella delivery ledger
status: To Do
assignee: []
created_date: '2026-08-31 21:45'
updated_date: '2026-08-31 21:45'
labels:
  - dogfood
  - github
  - task-queue
  - verification
dependencies:
  - TASK-80.3
  - TASK-80.4
priority: high
parent_task_id: TASK-80
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Use the Menantic-Creek-Capital/budget Lucella Domain Migration project as the live dogfood case. The open next work must be visible and correctly ordered while completed domain-cutover evidence remains linked and inspectable.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The queue shows GitHub issues 887, 881, 879, 877, 882, 876, and 880 with live delivery state and source links.
- [ ] #2 Closed issue 878 remains inspectable as delivered evidence but does not appear as next work.
- [ ] #3 GitHub issue, PR, check, merge, release, and deployment facts match GitHub at the recorded observation time.
- [ ] #4 A GitHub outage or insufficient token scope changes affected facts to Unknown without hiding the local work item.
<!-- AC:END -->
