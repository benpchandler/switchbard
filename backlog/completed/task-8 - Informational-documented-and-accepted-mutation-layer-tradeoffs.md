---
id: TASK-8
title: 'Informational: documented-and-accepted mutation-layer tradeoffs'
status: Done
assignee:
  - '@removal-fixes'
created_date: '2026-06-21 01:49'
updated_date: '2026-06-21 02:08'
labels:
  - mutation-arch
  - audit
  - informational
dependencies: []
priority: low
ordinal: 8000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Audit findings #6 (informational, accept-and-document, not fix). (a) save_config (app.rs:258-262) mutates self.config in memory first, then reports a persist failure via config_status with no rollback — acceptable for a local single-user app; the status line surfaces the failure. (b) onboarding::dismiss (crates/switchbard-gui/src/ui/onboarding.rs:385-388) mutates app.config.ui directly from a UI module then calls save_config — slightly muddy ownership but routes through the proper save path (no bypass). Resolution is to add brief code comments naming these as deliberate tradeoffs, not to add rollback/indirection machinery.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 A doc-comment on save_config names the in-memory-first / no-rollback behavior as a deliberate tradeoff for a local single-user app
- [x] #2 A brief comment at onboarding::dismiss notes the intentional cross-module config write routed through save_config
- [x] #3 No behavioral/code changes beyond comments unless a cheap improvement is obvious
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
No behavioral changes. Two doc-comment additions only.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Added two doc-comments documenting accepted tradeoffs: (1) on save_config in app.rs: explains the in-memory-first / no-rollback design is deliberate for a local single-user app, with config_status surfacing failures; (2) on onboarding::dismiss in ui/onboarding.rs: notes the intentional cross-module config write routes through save_config (no bypass), and names the ownership blur as an accepted pragmatic tradeoff for a UI-only flag. No behavioral changes. CI green.
<!-- SECTION:FINAL_SUMMARY:END -->
