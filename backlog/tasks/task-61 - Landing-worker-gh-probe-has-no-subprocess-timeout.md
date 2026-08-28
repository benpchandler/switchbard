---
id: TASK-61
title: 'Landing worker: gh probe has no subprocess timeout'
status: To Do
assignee: []
created_date: '2026-08-28 17:12'
labels: []
dependencies: []
priority: low
ordinal: 60000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
probe_pr_state (crates/switchbard-core/src/landing.rs:172) calls Command::output() with no timeout. A hung or interactively-prompting gh (e.g. mid re-auth) blocks the landing worker thread indefinitely for that entry - UI stays live (separate thread) but landing chips silently stop refreshing. Matches the repo's accepted no-wall-clock-kill posture (TASK-46), so advisory only. Found by adversarial audit of feat/landing-stage.
<!-- SECTION:DESCRIPTION:END -->
