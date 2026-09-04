---
id: TASK-142
title: 'sbt idea: organize by goal'
status: In Progress
assignee: []
created_date: '2026-09-03 21:34'
updated_date: '2026-09-04 09:47'
labels:
  - tui
  - idea
dependencies: []
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Filed from sbt 0.4.0 while at view=custom filter="" sort= selected=LED-641 pane=None.

Impact: organize by goal
Evidence: screen and action trail below, captured at filing time.

## Screen

```text
┌ budget  custom · paint:1 · group:project · 179/179 ──────────────────────────────────────────────────────────────────────────────┐
│1 id   2 status    3 pri 4 title                                                                                                  │
│▸ GitHub Actions Optimization · no def · 0/5                                                                                      │
│641    In Progress H     Nightly Full Suite: stop running the four macOS jobs every night main changes                            │
│643    To Do       H     Run the macOS jobs on the self-hosted music M1 runner instead of macos-26                                │
│642    To Do       M     Nightly dispatch: job-selector input and per-branch cancel-in-progress                                   │
│634    To Do       M     Speed up test-native-ios-uitests via build-for-testing reuse                                             │
│644    To Do       L     testmon-baseline: run on a schedule when main changed, not on every push                                 │
│▸ Personal Financial Statement · In Progress · 0/17                                                                               │
│649    To Do       H     Link the remaining institutions and sort every account into 413 categories in the owner's household      │
│649.1  To Do       H     Inventory Plaid coverage and cost for the nine missing institutions; owner decides link vs manual        │
│649.3  To Do       H     Account categorization: 413 category and owner attribution per account, with a sorting UI                │
│649.2  To Do       M     Owner links the supported institutions in production; agent verifies via read-only DSN                   │
│649.4  To Do       M     Owner enters manual lines for unlinkable accounts and sorts every account; totals reconcile to the target│
│648    To Do       H     Personal Financial Statement screen and export template (web + iOS, SBA Form 413 shape)                  │
│648.1  To Do       H     Initiative docs: strategy, architecture, plan for the PFS screen (web + iOS)                             │
│648.2  To Do       M     Backend: PFS statement aggregation with 413 section mapping and as-of date                               │
│648.3  To Do       M     Backend: manual statement lines and manual overrides for anything Plaid cannot link                      │
│648.4  To Do       M     Backend: open questions per line and answers that persist against a February baseline                    │
│648.5  To Do       M     Backend: persist the completed statement and export it as a lender-ready 413 (joint or separate)         │
│648.6  To Do       M     iOS: render the statement screen (sections, numbered lines, inline questions)                            │
│648.7  To Do       M     iOS: answer questions in place, pick the as-of date, and persist                                         │
│648.8  To Do       M     iOS: freeze and export the statement, then verify the whole journey on staging                           │
│648.9  To Do       M     Web: render the statement page (sections, numbered lines, inline questions)                              │
│648.10 To Do       M     Web: answer questions in place, pick the as-of date, and persist                                         │
│648.11 To Do       M     Web: freeze and export the statement, then verify the whole journey on staging                           │
│▸ no project                                                                                                                      │
│572    In Progress H     Alpha Tester Agreement + terms/privacy consistency + lawyer review of hold-harmless                      │
│571    In Progress H     Alpha launch: de-gate signups + RSU calculator audit CTA + capture/analytics                             │
│522    In Progress H     Calculator suite: TVM core + lease-vs-buy + HSA/FSA election designer + /tools                           │
│537    In Progress H     Checklist inference coverage: detect lease, insurance, bonus, student loans from transactions (rethink de│
│624    In Progress H     Group grants by owner so company is entered once per person                                              │
│582    In Progress H     Make agent staging verification a documented first-class path — persona infra exists but is undiscoverabl│
│652    In Progress H     One tax engine: migrate the public RSU calculator onto the post-auth household calculator and update it w│
│652.1  To Do       H     Extract the pure projection core and facts loader                                                        │
│652.2  To Do       H     Named projection components: federal, state, RSU gap, total, provenance                                  │
│652.3  To Do       H     State in the headline: liability, CA/NY/IL supplemental withholding, user override                       │
│652.4  To Do       H     Tax Surprise composes the named components; interrupt only on a true increase                            │
│652.6  To Do       H     Public calculator becomes an adapter over the core; parallel public math retired                         │
│652.7  To Do       H     Three-entry-point parity test and staging evidence                                                       │
│652.5  To Do       M     Shared fact mapping, W-4 Step 2, bonus default date, fail-closed tranches                                │
│652.8  To Do       M     Web tax workspace reads the named components                                                             │
│639    In Progress H     Open native signup: App Store discovery to account + household creation                                  │
│639.4  In Progress M     Native onboarding for a newly provisioned account (decision b = native, definition first)                │
│639.9  In Progress M     Staging proof: fresh identity signs in, draft persists, household projection equals the anonymous project│
│639.2  To Do       M     Production native-auth stack (Firebase prod project, env, migration, verify)                             │
│639.5  To Do       M     Interim: not-linked surface offers waitlist/request-access affordance                                    │
│647    In Progress H     Persist every transactional email send to a DB table for inspection                                      │
│561    In Progress H     Plaid Financial Insights onboarding capture contract                                                     │
│561.6  To Do       H     Execute Plaid Financial Insights production smoke evidence                                               │
│518    In Progress H     Prod data guardrails: agents never hold a writable prod credential                                       │
│538    In Progress H     Quick estimates: per-member income sections (spouse) + employer/income inference from payroll deposits   │
│397    In Progress H     Storybook data-volume personas: shared fixtures + workspace stories                                      │
│554    In Progress H     Tighten durable mutation boundary                                                                        │
│554.4  In Progress H     Convert income stream mutations to one service path                                                      │
│554.5  In Progress H     Route onboarding and tax import through income mutations                                                 │
│578    In Progress M     Add CI guard: fail the build when a Tailwind utility references an undefined design token                │
│526    In Progress M     Fix LedgerPercentageInput float display (0.57 renders as 56.99999999999999)                              │
│300    In Progress M     Follow-up: Use IncomeFrequency enum values in LinkedIn onboarding Core inserts                           │
│575    In Progress M     Reconcile backlog task statuses with shipped code (roadmap initiative statuses are downstream)           │
│401    In Progress L     Fix dependency inversion: move tax config to domain layer                                                │
│650    To Do       H     Account deletion must revoke the Sign in with Apple token and delete the Firebase user (App Review 5.1.1(│
│388    To Do       H     Add deterministic test hooks for onboarding checklist option selection                                   │
└──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────┘
:idea organize by goal▏
```

## Action trail

```text
session_start 0.4.0
action filter (0.0ms)
config_reload 0
action filter_cancel
action help (0.0ms)
action back (0.0ms)
unbound backspace
action command (0.0ms)
report Idea 141 in switchbard
action command idea (180.6ms)
action command (0.0ms)
```
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Reporter confirms the behaviour in sbt matches what they were trying to do
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
2026-09-04 shipped on feat/tui-goal-column: sbt gains a goal column. Membership derives from goals.yml (scope = project or label, attached tasks, attached projects) through one core predicate GoalDef::counts_task that the Digest actuals now also use. In sbt: :group goal sections tasks under each goal with this week's actual/target unit and pace in the heading (goals.yml order, no goal last); goal:<name> and goal:!<name> filter; the goal column shows via c or the column picker and paints as a link surface; the digit column menu offers Group on it. E2E tests in crates/switchbard-tui/tests/goal.rs (group headings, filter, column cell). Installed via mise run tui-install. AC1 waits on the reporter driving it in sbt.

2026-09-04 linking from sbt: a opens the goal panel for the selected task (✓ attached, · in scope already, pick toggles attach/detach and keeps the panel open); :goal <name> is the command form; ? lists a. Core change: manual goals now accept inputs as membership links, only tasks-measured goals count them toward the actual (attach_goal_inputs no longer refuses manual goals; core test updated). E2E: four new tests in crates/switchbard-tui/tests/goal.rs. Installed via mise run tui-install. To verify in budget: select a task, press a, pick a goal, then :group goal.
<!-- SECTION:NOTES:END -->
