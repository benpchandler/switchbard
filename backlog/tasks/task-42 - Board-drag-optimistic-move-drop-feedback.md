---
id: TASK-42
title: 'Board drag: optimistic move + drop feedback'
status: Done
assignee: []
created_date: '2026-08-20 02:27'
updated_date: '2026-08-20 13:11'
labels: []
dependencies: []
priority: high
ordinal: 42000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Dropping a card on another Board column gives no visual indication and lags 0.5-1.5s (backlog CLI subprocess round-trip before the cache reloads). Fix perceptually: render-time pending-move overlay so the card appears in the target column same-frame with a 'saving' treatment, landing flash on confirm, visible rollback on failure (error path must also reload the project cache), and a themed drop-target hover on dnd_drop_zone.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Dropped card renders in the destination column on the same frame, before the backlog CLI save resolves
- [ ] #2 Failed save visibly returns the card to its origin column and surfaces the error in the status line
- [ ] #3 Column under the pointer is visibly highlighted during a drag
- [ ] #4 Overlay never becomes a second source of truth: cleared on reload; concurrent worker reloads cannot strand a card
- [ ] #5 kittest coverage for optimistic render and failure rollback; mise run ci green
<!-- AC:END -->
