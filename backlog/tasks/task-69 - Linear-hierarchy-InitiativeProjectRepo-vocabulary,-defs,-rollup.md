---
id: TASK-69
title: 'Linear hierarchy: Initiative/Project/Repo vocabulary, defs, rollup'
status: To Do
assignee: []
created_date: '2026-08-31 12:36'
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
- [ ] #1 Every subtask Done; mise run ci green on macOS+Linux
- [ ] #2 Untouched task files stay byte-identical (write_layer_real_files green)
- [ ] #3 Legacy -m/--milestone/--clear-milestone and --project <DIR> invocations still work
- [ ] #4 docs/product-trajectory.md and CLAUDE.md updated
<!-- AC:END -->
