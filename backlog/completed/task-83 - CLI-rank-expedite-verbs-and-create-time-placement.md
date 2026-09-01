---
id: TASK-83
title: 'CLI: rank / expedite verbs and create-time placement'
status: Done
assignee: []
created_date: '2026-08-31 22:01'
updated_date: '2026-08-31 23:52'
labels:
  - backlog
  - cli
dependencies:
  - TASK-82
priority: high
project: Stack Ranking
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The switchbard-task surface for stack ranking (trajectory: 'Stack ranking'). Verb family: 'rank project <name>' and 'rank task <id>' with --top / --before <sibling> / --after <sibling>, plus 'unrank'; 'expedite <id>' / 'unexpedite <id>' for the exception lane. 'create' gains the same placement flags so a newly discovered task lands ranked among its siblings instead of reflexively expedited (owner insight: most queue-jumping was an incomplete queue). 'list', 'list --in-project', and 'project list' honor the computed order. Output contract stays agent-friendly: payload-only stdout, one-line stderr errors naming the next step.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 rank project/task with --top/--before/--after and unrank mutate ordering.yml through backlog/ordering.rs only
- [x] #2 expedite/unexpedite manage the lane; expediting an unknown or terminal task id errors with a next step
- [x] #3 create --rank-top/--rank-before/--rank-after places the new task among its siblings atomically with creation
- [x] #4 list and project list output follows the computed order (ranked first, unranked by existing comparator), covered by CLI tests
<!-- AC:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
rank_cmd.rs adds the verb families: 'rank project|task <X> --top|--before|--after <sibling>' (exactly one placement, clap-enforced), 'unrank project|task', 'expedite <ID>', 'unexpedite <ID>' - all printing 'Edited <X>' / 'no changes' per the existing edit-shaped contract, with bare/lowercase ids canonicalized through the same matcher view/edit use, and unrank/unexpedite accepting ids whose tasks are gone (stray-entry cleanup). 'create' gains --rank-top/--rank-before/--rank-after; on a rank failure the error names the already-created id. 'list' and 'project view' members ride the computed order from core; 'project list' leads with ranked projects then the name sort. Help text documents the ranking contract incl. the expedite-vs-rank-top guidance. 3 new process-boundary tests in tests/cli.rs; mise run ci green.
<!-- SECTION:FINAL_SUMMARY:END -->
