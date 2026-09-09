---
id: TASK-199
title: Make page viewport test follow the observed PR loading state
status: Done
assignee: []
created_date: '2026-09-08 17:34'
updated_date: '2026-09-08 18:12'
labels:
  - bug
  - test-infra
  - tui
dependencies: []
priority: medium
references:
  - https://github.com/benpchandler/switchbard/pull/145
  - https://github.com/benpchandler/switchbard/pull/144
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Impact: developers and users installing sbt can have a correct build rejected when a real background PR fetch finishes before the viewport assertion, blocking delivery nondeterministically. Evidence: root default-install gate on 2026-09-08, /tmp/switchbard-inbox-evidence/install-default.log:188, pages_render_at_small_and_large_terminal_sizes_with_no_tasks at crates/switchbard-tui/tests/pages.rs:109 expected Loading pull requests but the40x8 frame correctly showed Unavailable and a refresh failure notification. Existing fixture traverses multiple sizes while an actual non-git fixture lookup completes. Discovered during Inbox delivery integration; no duplicate found in sb list --all.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Viewport assertions cover all existing dimensions and selected-page headings without assuming the real PR worker remains loading.
- [x] #2 Loading and completed-unavailable expectations follow the observed state or explicit deterministic state setup; no sleeps or rerun-until-green masking.
- [x] #3 Targeted pages tests and final integrated TUI installation gate pass with the correction.
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Corrected pages.rs to assert the rendered body against the same App snapshot: Unavailable after an observed error, otherwise a verified loading state. All viewport/header checks retained. Targeted pages/navigation_badges/pr_refresh passed7 tests with2 existing live opt-ins ignored; the latter tests exercise loading, completed failure and retry explicitly. Evidence /tmp/switchbard-abstractions-evidence/task199-targeted.log. Final integrated installation criterion remains pending.

Subsequent delivery: combined PR144 externally merged into main6d0fa5bf on 2026-09-08T22:09:45Z. Its headabd06558 is tree-identical to validated6c97bd50 and passed6 checks plus1 scope skip. Earlier not-main note is superseded.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Fixed in d418be17: viewport body assertions follow observed App loading/error state while retaining all dimensions and heading checks. Targeted pages/navigation_badges/pr_refresh passed7 tests and exercise loading/failure/retry without lucky repeated runs. Default tui-install passed170 tests,21 opt-in ignored; log /tmp/switchbard-abstractions-evidence/install-default-final.log and binary SHA25682b143c1c8f9d7d0434cb91210f045f300c8a63bc5f3ace42637469f9afe48e0. Final PR145 head6c97bd50 passed6 CI checks with1 expected scope skip and is merged into Inbox PR144 branch, not main.
<!-- SECTION:FINAL_SUMMARY:END -->
