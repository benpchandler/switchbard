---
id: TASK-9
title: 'Harden git invocation: scrub GIT_* env, drop pre-push hook'
status: Done
assignee: []
created_date: '2026-06-21 03:55'
updated_date: '2026-06-21 03:56'
labels:
  - mutation-arch
  - reliability
dependencies: []
priority: high
ordinal: 9000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
GIT_DIR (and related GIT_* vars) exported by a pre-push hook invoked from a linked worktree override the -C <path> we pass to git, silently redirecting commands at the real repository instead of the target temp repo. This corrupted .git/config during a test run. Fix: route every git invocation through git_cmd() (crates/switchbard-core/src/git_env.rs), which scrubs GIT_DIR, GIT_INDEX_FILE, GIT_WORK_TREE, GIT_COMMON_DIR, GIT_OBJECT_DIRECTORY, and GIT_NAMESPACE before exec. Also remove the pre-push hook (.githooks/pre-push), its mise task (hooks:install), and the two CLAUDE.md references to it. ACs: (1) all Command::new(git) routed through git_cmd(), (2) helper scrubs the 6 GIT_* vars, (3) pre-push hook + mise task + docs removed, (4) mise run ci green.
<!-- SECTION:DESCRIPTION:END -->
