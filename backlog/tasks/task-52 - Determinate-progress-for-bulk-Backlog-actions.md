---
id: TASK-52
title: Determinate progress for bulk Backlog actions
status: Done
assignee: []
created_date: '2026-08-26 00:39'
labels: []
dependencies: []
priority: medium
ordinal: 52000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Both bulk workers set their status only when the whole batch finished, so a 43-task sweep was seconds of apparent nothing, indistinguishable from a hang. sync::Progress is a determinate channel alongside Status. Advances on failures too: it measures position in the batch, not how much worked - a bar stalling on a failing task reads as the hang it exists to dispel. Takes the buttons' place, since both actions mutate the same set through the same loop. Landed in PR #33.
<!-- SECTION:DESCRIPTION:END -->
