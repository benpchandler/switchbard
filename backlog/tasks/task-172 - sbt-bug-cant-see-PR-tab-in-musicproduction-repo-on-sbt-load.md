---
id: TASK-172
title: 'sbt bug: can''t see PR tab in musicproduction repo on sbt load?'
status: Done
assignee: []
created_date: '2026-09-08 11:24'
updated_date: '2026-09-09 12:12'
labels:
  - tui
  - bug
dependencies: []
priority: medium
project: Bugs
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Filed from sbt 0.4.0 while at view=v1 filter="" sort= selected=TASK-389 pane=None.

Impact: can't see PR tab in musicproduction repo on sbt load?
Evidence: screen and action trail below, captured at filing time.

## Screen

```text
┌ MusicProduction  v1 · hide:done · 245/249 ────────────────────────────────────────────────────────────────────────┐
│1 id   2 status    3 pri 4 title                                                                                   │
│389    In Progress H     T5d: EQ molten-cane mock-parity — edge overflow bug, cane richness, panel density + LOCKST│
│448    In Progress H     Provision Railway: service, persistent volume, secrets, deploy pipeline                   │
│20     In Progress M     P3-1a: External-trigger latent pre-sidechain restoration                                  │
│298    In Progress M     Unified Vary modal — build from locked design (7a/7c/7d/7e)                               │
│298.2  To Do       M     Vary modal — New-lane + entry point in the lane list                                      │
│298.3  To Do       M     Vary modal — dom_parity + layout_integrity fixtures for the redesign                      │
│298.1  To Do       L     Vary modal — aesthetic polish pass                                                        │
│328    In Progress M     Reverb: expose in insert picker + verify gates + re-curate presets                        │
│329    In Progress M     Build a Valhalla-quality algorithmic reverb engine                                        │
│399    In Progress M     Make Trig.notes[] audible — poly voice path in pattern-runtime                            │
│438    In Progress M     AuthBootstrapHosted policy-change test flakes under CI load in the blocking fast ship gate│
│112    To Do       H     Sample Picker: contextual fit ranking and user-facing why                                 │
│149.2  To Do       H     Kick Synth: sample-and-hold AHD env params at trigger                                     │
│149.3  To Do       H     Kick Synth: pin AHD decay seconds to T60 with a numerical test                            │
│149.9  To Do       H     Kick Synth: connect UI waveform scope to actual DSP                                       │
│150    To Do       H     Lyon Sampler v0: single-zone chromatic sampler (UX-first)                                 │
│150.1  To Do       H     Sampler v1 artifact package                                                               │
│150.2  To Do       H     Sampler explicit instrument identity                                                      │
│150.3  To Do       H     Sampler inspector renderer lifecycle                                                      │
│150.4  To Do       H     Sampler root-aware chromatic playback                                                     │
│150.5  To Do       H     Sampler envelope and polyphony seal                                                       │
│150.6  To Do       H     Sampler filter runtime and persistence                                                    │
│150.10 To Do       H     Sampler missing sample and project lifecycle                                              │
│150.11 To Do       H     Sampler ship gate integration                                                             │
│150.7  To Do       M     Sampler loop tab v1                                                                       │
│150.8  To Do       M     Sampler mod tab v1 stored state                                                           │
│150.9  To Do       M     Sampler preset round-trip                                                                 │
│263    To Do       H     Bundle active-project pointer is machine-global — concurrent Lyon instances clobber each o│
│266    To Do       H     Lock-and-Fill Opinion Engine — percussion v1                                              │
│266.1  In Progress H     Step 1 — See what's there: render locked loop → spectrogram Analysis tab                  │
│266.2  In Progress H     Step 2 — See the space: overlay open time slots + frequency bands                         │
│266.3  To Do       H     Step 3 — Suggest a sound: catalog-pick into the biggest frequency hole                    │
│266.4  To Do       H     Step 4 — Suggest a pattern: place the sound into open time slots                          │
│266.5  To Do       H     Step 5 — Lock-and-fill in the trig UI (the real surface)                                  │
│270    To Do       H     Calibrate FitnessEvaluator weights/thresholds on real STFT data                           │
│273    To Do       H     Trig-level lock + same-instrument fill/variations (the poker-card primitive)              │
│283    To Do       H     Analysis space-map from catalog sound-profiles (reuse) + on-demand fallback               │
│291    To Do       H     UI design for pattern / groove / variant surfaces (tab strip + libraries + per-lane varian│
│299    To Do       H     Drum-foundation preset should load a SOUNDING kit, not silent lanes                       │
│306    To Do       H     Lyon: curated genre ensembles (solid rack of cool sounds)                                 │
│308    To Do       H     Lyon: make sample-choice discoverable (move out from under 'Amp'/'Device')                │
│315    To Do       H     Storybook currency — bring stories up to date with the app, component-by-component, anchor│
│320    To Do       H     Host Bodies (James) discovery interview — synthesis & strategic insights                  │
│380    To Do       H     Ship-gate playwright budgets stale; audio leg times out at 30 min                         │
│388    To Do       H     T2b: instrument pieces mock-parity — header, filter viz, glass treatment + LOCKSTEP gate  │
└───────────────────────────────────────────────────────────────────────────────────────────────────────────────────┘
:bug can't see PR tab in musicproduction repo on sbt load?▏
```

## Action trail

```text
session_start 0.4.0
unbound tab
action command (0.0ms)
```
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Reporter confirms the behaviour in sbt matches what they were trying to do
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Confirmed fixed, and the cause was a fleet accident rather than a code defect.

The trail is 'session_start 0.4.0' then 'unbound tab' - the key was not bound, so that build had no Pull Requests page at all. The telemetry log explains it: at 11:23:39 all four running sbt processes logged self_restart and re-execed into a freshly installed binary, and 'unbound tab' starts three seconds later. A `cargo install` from a worktree without the PR page had downgraded every open session. At 11:26:02 they all restarted again into a build that had it, which is why it looked fixed on its own.

Nothing in the code gates the tab on the repo: draw_navigation always renders both chips and default.lua always binds tab, in any repo, with or without a GitHub remote.

Evidence: crates/switchbard-tui/tests/pages.rs::the_pull_requests_tab_is_on_the_first_frame_of_a_repo_with_no_remote asserts both page chips, the 'tab switch page' hint and a working tab press on the very first frame of a repo that has no remote at all.

The underlying hazard is real and outlives this task: sbt re-execs into whatever binary replaces it, including one built from an older branch, and reports version 0.4.0 either way - there is no way to tell which build you are on. That is also what caused TASK-173. Worth a follow-up if it bites again.

Recurrence on 2026-09-09, and the follow-up this task's notes said was worth doing.

The page vanished again by the same mechanism: the installed sbt had been built from /Users/bpc/Dev/.worktrees/switchbard-abstractions (a worktree since deleted), and cargo's install record was the only surviving trace of where it came from. The same install had also replaced sb with a build from the primary checkout parked on feat/tui-live-work.

Fixed mechanically rather than by discipline (PR #149):
- crates/switchbard-core/build.rs stamps commit/branch/dirty at compile time; switchbard_core::build_identity is the single reader, surfaced by --version, a build-id subcommand on both binaries, and sbt's session_start event. 'which build am I on' is now answerable, which it was not when this task was first investigated.
- scripts/install-switchbard.sh refuses any install whose target tree does not contain the commit the installed binary was built from, and refuses an unverifiable stamp. --force overrides and names what is dropped. mise run install / tui-install route through it.
- scripts/test-install-guard.sh covers five cases including this exact shape, confirmed to fail when the ancestry check is removed, and runs in test-developer-gates.sh.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Confirmed fixed. The PR tab was missing because a cargo install from a branch without the PR page downgraded all four running sbt sessions at 11:23:39; a later install restored it. No code gates the tab on the repo, and a first-frame test now locks that. The standing hazard - sbt silently re-execs into any replacement binary and always reports 0.4.0 - is named in the notes.
<!-- SECTION:FINAL_SUMMARY:END -->
