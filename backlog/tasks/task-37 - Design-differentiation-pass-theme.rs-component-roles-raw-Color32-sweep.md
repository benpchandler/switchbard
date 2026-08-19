---
id: TASK-37
title: 'Design differentiation pass: theme.rs component roles + raw-Color32 sweep'
status: Done
assignee: []
created_date: '2026-08-05 18:26'
updated_date: '2026-08-05 18:26'
labels:
  - backlog
  - ux
  - design
dependencies: []
priority: high
ordinal: 37000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Owner: 'everything feels dominated by gray — differentiate text, input fields, navigation areas, highlighted items.' Extend theme.rs with component-level semantic roles (input fill, nav strip, rail tier, card surface split from input surface) and sweep ui/** for hand-rolled Color32 constants, centralizing them into theme.rs accessors.
<!-- SECTION:DESCRIPTION:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Palette hierarchy widened (was near-invisible in dark mode: faint_bg/panel_fill/card_bg spanned only #1C1A17->#221F1B->#2B2721, a ~9-value swing per channel; now #171512->#221F1B->#37312A). Split the conflated 'card surface' vs 'input surface' concepts: new card_bg() (flight-strip/digest cards, Statistics burndown chart -- what used to read ui.visuals().extreme_bg_color directly) is now distinct from what apply() feeds into egui's actual extreme_bg_color slot (repointed to faint_bg -- a sunken 'type into' field, the opposite metaphor from a raised card). Added a visible focus ring for text inputs (sky()-colored 1.5px border on hover/active, muted border at rest) via global Visuals styling -- zero call-site changes, benefits every TextEdit in the app. New nav_bg() wraps the top-bar view-tab strip and the Backlog lens-tab strip in their own background band, distinct from surrounding content. New rail_bg() gives the persistent detail rail (TASK-34) its own third workspace tier instead of inheriting the board's panel_fill. Swept raw Color32 usage: 6 hand-rolled Color32::GRAY/DARK_GRAY 'idle dot' call sites (sidebar.rs, workspace/mod.rs, agent_context.rs) centralized into new theme::idle_dot() (reuses muted_text(), so it now actually shifts with the theme); top_bar.rs's error label switched from raw Color32::RED to theme::danger(); onboarding.rs's two duplicated white-text-on-green buttons centralized into a new theme::success_button() -- which surfaced and fixed a REAL, previously-untested WCAG AA failure: theme::green() as a button fill only clears ~2.25:1 contrast with white text in dark mode (needs 4.5:1), since no legibility_audit fixture had ever exercised the onboarding overlay. Added two onboarding fixtures (empty-results and repo-picker panes, both themes) to legibility_audit.rs, seeding DiscoveryState::Ready directly so the harness never triggers a real filesystem scan of /Users/bpc. Sanity-checked the fix is real by temporarily reverting to the buggy fill and confirming the audit fails with the exact predicted ~2.3:1 ratio, then restoring it. Scope note: List's/Milestones' row selection still uses stock egui .selected()/selectable_label() fills rather than Board's proven-AA-safe stroke-based approach -- a real remaining inconsistency the owner's 'selection treatment consistent everywhere' ask calls out, deliberately left for a follow-up pass (touches row layout in both files, no evidence of an active AA failure there today, and this session was already large) rather than a rushed tack-on here.
<!-- SECTION:NOTES:END -->
