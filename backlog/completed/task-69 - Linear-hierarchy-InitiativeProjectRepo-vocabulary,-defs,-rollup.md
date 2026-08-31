---
id: TASK-69
title: 'Linear hierarchy: Initiative/Project/Repo vocabulary, defs, rollup'
status: Done
assignee: []
created_date: '2026-08-31 12:36'
updated_date: '2026-08-31 14:04'
labels: []
dependencies: []
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Adopt Linear's four-tier hierarchy (Initiative - Project - Issue - Sub-issue) across core, CLI, and GUI. Decision record: docs/product-trajectory.md, 'Linear-vocabulary hierarchy' entry (owner-approved 2026-08-31). Membership key migrates milestone: -> project: lazily; optional def files backlog/projects/ + backlog/initiatives/; rollup computed never stored; repo tier renamed Repo user-facing with --project <DIR> as deprecated alias of --repo. Deferred (ask first): project rename, GUI def authoring, case-insensitive name merge.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Every subtask Done; mise run ci green on macOS+Linux
- [x] #2 Untouched task files stay byte-identical (write_layer_real_files green)
- [x] #3 Legacy -m/--milestone/--clear-milestone and --project <DIR> invocations still work
- [x] #4 docs/product-trajectory.md and CLAUDE.md updated
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
PR: https://github.com/benpchandler/switchbard/pull/55
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Linear hierarchy shipped end to end in PR #55 (merged 8d7dfd2): Initiative/Project/Issue/Sub-issue vocabulary, lazy milestone->project key migration, def files + PROJECT_STATUSES, computed rollup, CLI verb families + six-column list, GUI Projects lens driven by the shared rollup, full legacy compatibility (flags, saved views, facet keys, task files). CI green on macOS+Linux per commit; 7 code-review findings fixed pre-merge; skill file and installed binary refreshed post-merge.
<!-- SECTION:FINAL_SUMMARY:END -->
