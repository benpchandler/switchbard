---
id: TASK-171
title: 'sbt bug: on PR 481, the app incorrectly reported that the PR wasn''t mergeable even though it was.'
status: Done
assignee: []
created_date: '2026-09-08 11:20'
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
Filed from sbt 0.4.0 while at page=PullRequests view=v1 filter="" sort= selected=TASK-7 pane=None.

Impact: on PR 481, the app incorrectly reported that the PR wasn't mergeable even though it was.
Evidence: screen and action trail below, captured at filing time.

## Screen

```text
  Tasks   [Pull Requests]  tab switch page
PR #481: state: Merged                                                                                  2 · n dismiss
┌ Pull Requests ────────────────────────────────────────────────────────────────────────────────────────────────────┐
│Menantic-Creek-Capital/Lyon · Observed 13s                                                                         │
│100/100 shown · PARTIAL · :more                                                                                    │
│filter: all                                                                                                        │
│                                                                                                                   │
│1 id    2 State 3 Tasks      4 Checks       5 title                                                                │
│#481    Merged  -            NF             test(lyon): gate measured browser journeys and unblock catalog search  │
│#480    Merged  -            NF             fix(lyon): rebuild stale AuthBootstrapHosted bundle (ship gate red on m│
│#479    Merged  -            NF             ci(lang-gates): cancel superseded runs; cache uv in every job          │
│#478    Merged  -            NF             fix(lyon): restore same-origin OAuth redirects                         │
│#477    Merged  -            NF             build: pin Bun 1.4.0 in .tool-versions and enforce it in the bundle bui│
│#476    Merged  -            NF             fix(verify): add Firebase helper route to hosted inventory, refresh bun│
│#475    Merged  -            NF             fix(lyon): serve Firebase auth helper init.json without Firebase Hostin│
│#474    Merged  -            NF             feat(lyon): same-origin Firebase auth helper proxy                     │
│#473    Merged  -            NF             fix(lyon): hosted sign-in via signInWithPopup                          │
│#472    Merged  -            NF             fix(deploy): replace curl --aws-sigv4 with stdlib SigV4 helper (bookwor│
│#471    Merged  -            NF             chore(deploy): widen healthcheck window for whole-catalog first boot   │
│#470    Merged  -            NF             fix(lyon): remove notes by double-click through chord overlays         │
│#469    Merged  -            NF             chore: untrack firebase-debug.log CLI artifact                         │
│#468    Merged  -            NF             fix(deploy): snapshot ATTACH needs a URI-mode connection               │
│#467    Merged  -            NF             fix(lyon): deflake AuthBootstrapHosted policy-change assertion (task-43│
│#466    Merged  -            NF             fix(bundle): thread request context through OpenAudio into object fetch│
│#465    Merged  -            NF             feat(bundle): whole-catalog hosted publication - S3 audio backend, join│
│#464    Merged  -            NF             feat(deploy): Hosted Lyon v1 - Railway image, entrypoint, starter catal│
│#463    Merged  -            NF             Require double-click to remove notes                                   │
│#462    Merged  -            NF             chore(backlog): land stranded task edits, file 3 cleanup follow-ups    │
│#461    Merged  -            NF             chore: land three stranded worktree pieces (mutmut advisory, seam e2e r│
│#460    Merged  -            NF             docs(research): land stem-separation seal evidence + P3-1a-v2 decision │
│#459    Merged  -            NF             docs(shell): Slice 2 Project-Home wiring handoff + TASK-351            │
│#458    Merged  -            NF             fix(lyon): name the noun on both swap controls (task-307)              │
│#457    Merged  -            NF             chore(backlog): archive completed tasks + file tasks 430-432           │
│#456    Merged  -            NF             docs: route task writes through switchbard-task                        │
│#455    Merged  -            NF             chore(lyon): retire the Roadmap subtab                                 │
│#454    Merged  -            NF             test(lyon): hoist operability, sweep it over every surface (+2 a11y def│
│#453    Merged  -            NF             test(lyon): surface-discovered invariant sweeps (+ raw genre token fix)│
│#452    Merged  -            NF             feat(lyon): prepare verified multi-tenant cloud launch                 │
│#451    Merged  -            NF             fix(ci): bound cold collaboration setup                                │
│#450    Merged  -            NF             Ship hosted collaboration foundation safely                            │
│#449    Merged  -            NF             feat(lyon): overhaul creation and sound workflows                      │
│#448    Merged  -            NF             fix(lyon): correct auto gain placement and sampler release             │
│#447    Merged  -            NF             fix(arrangement): play clip edits without refreshing                   │
│#446    Merged  -            NF             feat(lyon): add capability-based chromatic Notes editing               │
│#445    Merged  -            NF             feat(lyon): add curated percussion kits                                │
│#444    Merged  -            NF             feat(lyon): ship polysynth and improve workstation performance         │
│#443    Merged  -            NF             Ship React Project Home and create flow                                │
└───────────────────────────────────────────────────────────────────────────────────────────────────────────────────┘
:bug on PR 481, the app incorrectly reported that the PR wasn't mergeable even though it was. ▏
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
action down (0.0ms)
action down (0.0ms)
action down (0.0ms)
action down (0.0ms)
action down (0.0ms)
action down (0.0ms)
action down (0.0ms)
action down (0.0ms)
action down (0.0ms)
action down (0.0ms)
action down (0.0ms)
action page (0.0ms)
action merge (0.0ms)
action group (0.0ms)
action down (0.0ms)
action up (0.0ms)
action open (0.0ms)
action back (0.0ms)
action reload (0.0ms)
action command (0.0ms)
error say what you were trying to do: :bug <text>
action command bug (0.1ms)
action command (0.0ms)
```
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Reporter confirms the behaviour in sbt matches what they were trying to do
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Reproduced from the record, then live.

What happened: the telemetry log has 'action merge' at 11:18:43. GitHub says Lyon #481 was merged at 2026-09-08T15:19:20Z = 11:19:20 local, 37 seconds later, by the reporter on github.com. So the PR was open and mergeable when sbt refused it.

Cause: its check rollup was FAILURE with fifteen check runs, none of them isRequired - i.e. mergeStateStatus = UNSTABLE, which is a state GitHub's own merge button stays live in. sbt's eligible() demanded mergeStateStatus == CLEAN and returned 'GitHub has not confirmed merge readiness (MERGEABLE/UNSTABLE)'.

Fix: accept GitHub's own allowed set - CLEAN, HAS_HOOKS, UNSTABLE - and move the guard to the confirmation, which now names the readiness state ('Checks: not all green (UNSTABLE) - none of them is required, so GitHub allows this merge'). BLOCKED, BEHIND, DIRTY, DRAFT and UNKNOWN still refuse. Also: GitHub answers mergeable=UNKNOWN on the first read of a PR it has not looked at lately and starts computing behind that answer, so prepare() now re-observes up to 3 times before believing it.

Evidence, live against benpchandler/janus#88 (MERGEABLE/UNSTABLE, read-only, nothing submitted):
- before: eligible=false, Err("GitHub has not confirmed merge readiness (MERGEABLE/UNSTABLE)") - the reported message, reproduced
- after:  eligible=true with the caveat line, and the rendered confirmation shows it above the method choices
Tests: pr_merge/tests.rs::states_github_itself_merges_on_are_allowed_and_each_names_its_caveat, ::an_unstable_pr_prepares_and_its_confirmation_carries_the_reason, ::pending_mergeability_is_asked_again_rather_than_reported_as_a_refusal, ::live_open_pr_verdict_matches_what_github_permits (ignored, live), and tui tests/pr_merge.rs::live_confirmation_names_a_non_clean_readiness_state (ignored, live).

Decision to review: loosening the gate is a safety-posture change. It was proposed to the owner and taken as the default when no answer came - the guard is now the human reading the named state, not sbt refusing a merge GitHub permits. Reverting is a one-line change to MERGE_READY in pr_merge/observe.rs.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
sbt refused Lyon #481 because its merge gate demanded mergeStateStatus == CLEAN, while the PR was MERGEABLE/UNSTABLE - checks failing, none of them required - which GitHub itself merges. The gate now accepts GitHub's allowed set and the confirmation names the readiness state, so the guard is explicit human approval with the reason in view. A first UNKNOWN answer is also retried rather than reported as a refusal. Fixed on fix/tui-bug-sweep (370c2d1).
<!-- SECTION:FINAL_SUMMARY:END -->
