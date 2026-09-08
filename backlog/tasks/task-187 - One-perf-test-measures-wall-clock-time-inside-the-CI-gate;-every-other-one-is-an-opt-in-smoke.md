---
id: TASK-187
title: One perf test measures wall-clock time inside the CI gate; every other one is an opt-in smoke
status: To Do
assignee: []
created_date: '2026-09-08 14:31'
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
- [ ] #1 The test either no longer runs in the default gate, or its assertion no longer depends on how busy the machine is
- [ ] #2 Whichever way it goes, the reason is written next to the test, so the next person does not have to rediscover that every other perf test in the repo is ignored
- [ ] #3 mise run test stays green across five consecutive runs while the machine is under load (the condition that surfaced this)
<!-- AC:END -->
