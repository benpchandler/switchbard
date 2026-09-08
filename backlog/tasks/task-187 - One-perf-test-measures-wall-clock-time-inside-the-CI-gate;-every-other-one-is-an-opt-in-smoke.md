---
id: TASK-187
title: One perf test measures wall-clock time inside the CI gate; every other one is an opt-in smoke
status: Done
assignee: []
created_date: '2026-09-08 14:31'
updated_date: '2026-09-08 15:41'
labels:
  - perf
  - test-infra
  - ci
dependencies: []
priority: low
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
One test asserts a stopwatch reading while the normal test gate runs:

  crates/switchbard-gui/tests/mission_command_sidecar.rs:498
  mission_controls_50_row_p95_is_within_33ms

It renders 50 mission rows 200 times and fails if the 95th-percentile frame takes longer than 33ms. That is a real thing to care about, but wall-clock time on a shared runner measures how busy the machine is as much as how fast the code is - so it can go red for reasons that have nothing to do with the change under review.

Every other timing test in this repo is already opted out of the gate for exactly that reason, with the same wording:

  #[ignore = "perf smoke - run explicitly, see module doc"]
  agent_hooks_perf_smoke.rs, digest_perf_smoke.rs, dispatch_chrome_perf_smoke.rs,
  projects_rank_perf_smoke.rs, tasks_place_perf_smoke.rs, workspace_perf_smoke.rs

This one is the lone exception, and it looks like an oversight rather than a decision - mission_command_view.rs:358 in the same area IS ignored, described as a '50-row render perf smoke'.

Impact: whoever runs the gate on a loaded machine. A red CI that has nothing to do with the change is what trains people to re-run instead of read, which is the cost TASK-181 already charged once.

Evidence: seen once, at 35.995ms against the 33ms budget, while five full test suites were deliberately running at the same time (2026-09-08, during the TASK-180/181 work). On an idle machine it comes back in about 1.6ms per frame - roughly twenty times inside budget - so it is nowhere near the line under normal conditions. It has not failed on real CI to date.

Options - this needs a decision, not a default:

(a) Mark it #[ignore] like every other perf smoke. Consistent, one-line change, but the budget then only gets checked when someone runs it on purpose. The perf ledger (docs/perf/README.md) is where that discipline already lives.

(b) Keep it in the gate and make the measurement load-proof - measure work done rather than seconds elapsed (frame count, allocations, or CPU time rather than wall clock). Keeps a real signal on every PR; more work, and needs a metric that is actually stable.

(c) Keep it and widen the budget. Cheapest, and the worst of the three: it weakens the check without making it trustworthy, and the next loaded runner just moves the line again.

Recommend (a) unless the 33ms number is load-bearing for someone, in which case (b).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 The test either no longer runs in the default gate, or its assertion no longer depends on how busy the machine is
- [x] #2 Whichever way it goes, the reason is written next to the test, so the next person does not have to rediscover that every other perf test in the repo is ignored
- [x] #3 mise run test stays green across five consecutive runs while the machine is under load (the condition that surfaced this)
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Owner asked why this was running at all. Two answers, and the second changes the task.

First, it is not a TUI test. Mission Control does not exist in sbt - the only 'mission' match anywhere under crates/switchbard-tui/src is the word 'submission' in a doc comment, and the crate has no mission test files. This is a switchbard-gui test, for the egui desktop app where Mission Command lives as Place::Missions. It showed up in my runs only because mise run test is cargo test --workspace and I was running the whole suite repeatedly while chasing the TUI flakes.

Second, and better: it is a near-duplicate of a test that is ALREADY an opt-in smoke.

  crates/switchbard-gui/tests/mission_command_view.rs:358
  mission_command_fifty_row_perf_smoke - #[ignore]d
  50 rows, 200 samples, p95 < 33.0

  crates/switchbard-gui/tests/mission_command_sidecar.rs:498
  mission_controls_50_row_p95_is_within_33ms - was running in the gate
  50 rows, 200 samples, p95 <= 33.0

Same place, same fixture size, same sample count, same budget, same method. So this was never really a three-way decision - it was one measurement with two homes, one of which had been opted out of the gate and one of which had not. Option (b) has no case to answer while the twin exists, and (c) was always the worst.

Fix: the gate-running copy now carries the same #[ignore] as its twin, with the duplication named in a doc comment above it so the next person does not re-derive this. Coverage is unchanged - the twin still measures the same budget - and both still run on demand with --ignored.

Verified: mise run ci green (85 test binaries, 0 failures); the test reports 'ignored' in the ordinary run and still measures on demand (MISSION_SIDECAR_P95_MS=0.972 idle, ~34x inside the 33ms budget).

Reversible in one line if the 33ms number turns out to be load-bearing where the twin cannot see it: delete the #[ignore].
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Not a TUI test at all - Mission Control is absent from sbt; this is a switchbard-gui test that ran because mise run test is a workspace run. The owner's question surfaced the real point: it is a near-duplicate of mission_command_fifty_row_perf_smoke, which was already an opt-in smoke - same place, 50 rows, 200 samples, 33ms budget. The gate-running copy now carries the same #[ignore] as its twin, with the duplication named above it. Coverage unchanged, both still run with --ignored, mise run ci green.
<!-- SECTION:FINAL_SUMMARY:END -->
