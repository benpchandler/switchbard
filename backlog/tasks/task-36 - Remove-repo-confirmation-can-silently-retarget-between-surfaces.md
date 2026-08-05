---
id: TASK-36
title: Remove-repo confirmation can silently retarget between surfaces
status: To Do
assignee: []
created_date: '2026-08-05 18:15'
labels:
  - ux
  - hardening
dependencies: []
priority: low
ordinal: 36000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Verifier finding (c7e6624, LOW, UX-only): with Settings open in the Servers view, clicking Remove on the other surface while a confirmation is pending silently retargets the shared confirm_remove_repo Option. No data risk (single field, idempotent removal). Options: block second trigger while pending, or surface a target-changed hint.
<!-- SECTION:DESCRIPTION:END -->
