---
id: TASK-155
title: 'sbt bug: in progress row 3 fading to brown / flashing brown during loop'
status: Done
assignee: []
created_date: '2026-09-04 12:31'
updated_date: '2026-09-08 12:40'
labels:
  - tui
  - bug
dependencies: []
priority: medium
project: Bugs
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Filed from sbt 0.4.0 while at view=custom filter="" sort= selected=TASK-150 pane=None.

Impact: in progress row 3 fading to brown / flashing brown during loop
Evidence: screen and action trail below, captured at filing time.

## Screen

```text
┌ switchbard  custom · cols:id,status,priority,title,rank · hide:done · group:project · working:1 · 56/117 ────┐
│1 id 2 status    3 pri 4 title                                                                             5 #│
│▸ top · 3                                                                                                     │
│80   To Do       H     Make the Task Queue aware of GitHub delivery state                                  1  │
│141  To Do       M     sbt idea: PR page with actions and status notifications across pages                2  │
│150  In Progress H     Live work marker: sbt blinks the rows an agent session is working; sb work claim/re 3  │
│▸ EGUI Polish · Planned · 0/2                                                                                 │
│78   To Do       H     Elevation scale tokens in theme.rs                                                     │
│79   To Do       M     Sweep surfaces onto the elevation scale                                                │
│▸ GitHub Operations · Planned · 0/6                                                                           │
│115  To Do       H     Lock the GitHub Operations authority, placement, and command contract                  │
│116  To Do       H     Extend GitHub delivery observations for repository pull-request operations             │
│117  To Do       H     Build the canonical Ops > Pull requests surface                                        │
│118  To Do       H     Add guarded pull-request review operations                                             │
│119  To Do       H     Add guarded CI, branch-update, and merge operations                                    │
│120  To Do       H     Dogfood and release GitHub Operations in the native app                                │
│▸ Instant Cold Start · Planned · 0/6                                                                          │
│121  To Do       H     Lock the instant-startup contract and failing first-frame journey                      │
│122  To Do       H     Build the bounded sharded startup snapshot kernel                                      │
│123  To Do       H     Render Servers immediately from last-known topology and processes                      │
│124  To Do       H     Render Workspace immediately from last-known Git and worktree state                    │
│125  To Do       H     Render Tasks and Dispatch immediately from last-known read models                      │
│126  To Do       M     Unify Agents caching and close the cross-platform startup gates                        │
│▸ no project                                                                                                  │
│80.3 To Do       H     Build the Task Queue GitHub delivery backend                                           │
│80.4 To Do       H     Render mixed local and GitHub-backed work in the Task Queue                            │
│80.1 To Do       H     Prove the Task Queue with the Lucella delivery ledger                                  │
│39   To Do       M     Reap dispatch runs orphaned by an app restart                                          │
│81   To Do       M     RemovalAuthorization: make the force gate a domain type, not a caller-supplied bool    │
│38   To Do       M     Unify List/Milestones row selection with Board's stroke-based indicator                │
│13   To Do       L     Virtualize Backlog task list rows for large repo/task counts                           │
│61   To Do       L     Landing worker: gh probe has no subprocess timeout                                     │
│31   To Do       L     Tombstone filename collides on same-second consecutive wipes                           │
│36   To Do       L     Remove-repo confirmation can silently retarget between surfaces                        │
│68   To Do       L     Format fork: diverge on named wins                                                     │
│87   To Do       H     Digest Tab: Clickable tasks                                                            │
│110  To Do       H     Goal check-in drafts survive week rollover with stale values                           │
│137  To Do       H     Owner cannot discover what is waiting on them without being told in chat               │
│139  To Do       H     Owner cannot see at a glance which tasks an agent session is actively working          │
│152  To Do       H     sbt: paint_auto stores a palette token, not a resolved hex                             │
│153  To Do       H     sbt: resume the last view on launch, not just across self-restart                      │
│93   To Do       M     Give SB ability to detect refactoring candidates                                       │
│94   To Do       M     Enable "integrations' vs hardcoded / config.                                           │
│95   To Do       M     sb add <title>: quick capture that falls back to the hub repo outside a Backlog rep    │
│104  To Do       M     Create sprint from tasks / goals / projects                                            │
│108  To Do       M     TASK-56's cross-thread repaint race recurs in other backlog_controls.rs tests          │
│109  To Do       M     Retire the unreachable legacy Backlog lenses                                           │
│113  To Do       M     Support-request store for Command (NEEDS_DECISION/SITREP)                              │
│136  To Do       M     sbt: prior text on screen shows after terminal app quit and reload                     │
│140  To Do       M     Id column truncation makes distinct ids look identical; the repeated repo prefix wa    │
│143  To Do       M     sbt idea: when an idea comes in just start building it                                 │
└──────────────────────────────────────────────────────────────────────────────────────────────────────────────┘
:bug in progress row 3 fading to brown / flashing brown during loop▏
```

## Action trail

```text
action down (0.0ms)
action down (0.0ms)
action down (0.0ms)
action down (0.0ms)
action down (0.0ms)
action down (0.0ms)
action down (0.0ms)
action up (0.0ms)
config_reload 0
action reload (13.9ms)
action task (0.0ms)
action down (0.0ms)
action down (0.0ms)
action up (0.0ms)
action up (0.0ms)
action task (0.0ms)
action rank 3 TASK-150
unbound h
unbound i
action sort_column (0.0ms)
config_reload 0
action reload (5.9ms)
action paint
action paint (0.0ms)
action paint_clear_all
action paint
action paint (0.0ms)
action paint_clear_all
unbound backspace
action command (0.0ms)
```
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Reporter confirms the behaviour in sbt matches what they were trying to do
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Reproduced end-to-end in the sbt harness: with a live work claim and a 40ms pulse, the row's text sampled (202,132,41) at the trough against a (244,159,49) rest colour - a darkened amber is brown, which is exactly what was reported.

Cause: Theme::working_fg swung the text symmetrically, toward white at the peak and toward BLACK in the trough (WORKING_TEXT_SWING). Darkening a warm foreground is what makes it brown; there is no way to dim berg's #f49f31 without browning it.

Fix: the band already carries the dark half of the pulse (its RGB fades to black and disappears below 4% glow), so the text now only ever lifts toward white and sits at its own rest colour in the trough. Constant renamed WORKING_TEXT_LIFT.

Evidence: crates/switchbard-tui/tests/work.rs::the_pulse_never_darkens_a_working_rows_text_below_its_rest_colour asserts no frame of a 200-sample pulse is dimmer than a resting row's text, and that the trough is the rest colour exactly. Verified failing before the fix with the message 'no frame of the pulse is dimmer than rest: (202, 132, 41) vs (244, 159, 49)'.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
The working-row pulse dimmed the row's text toward black, and a darkened amber reads as brown. The band carries the dark half of the pulse on its own, so the text now only brightens - it rests at its own colour in the trough and lifts toward white at the peak. Fixed on fix/tui-bug-sweep (370c2d1).
<!-- SECTION:FINAL_SUMMARY:END -->
