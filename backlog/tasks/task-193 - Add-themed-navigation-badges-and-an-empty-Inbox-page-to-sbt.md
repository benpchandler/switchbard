---
id: TASK-193
title: Add themed navigation badges and an empty Inbox page to sbt
status: In Review
assignee: []
created_date: '2026-09-08 16:01'
updated_date: '2026-09-08 18:12'
labels:
  - tui
  - feature
  - ball:me
dependencies: []
priority: high
references:
  - https://github.com/benpchandler/switchbard/pull/144
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Impact: owners lose visibility of pending work and open PRs while navigating filtered task lists. This first high-priority slice establishes a persistent Inbox destination and themed navigation counts. Evidence: owner discussion on 2026-09-08 following seven abstraction tasks marked complete before a PR existed. Scope: PR badge counts all open PRs independently of list filters; orange theme badge with black text; blank Inbox destination; no collapsible pane or invented action data yet.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 First-row Pull Requests badge counts all open PRs for the repo independently of filters and remains visible on other pages; zero hides the badge.
- [x] #2 A shared semantic attention_badge Lua theme surface defaults to orange background and black numerals and supports hot reload.
- [x] #3 Inbox is reachable from first-row navigation and keyboard help, renders an honest empty placeholder, and cannot invoke hidden task or PR mutations.
- [x] #4 Loading, failed refresh, navigation, narrow layouts, and zero or many PRs are covered through rendered E2E checks and actual sbt use.
- [x] #5 Changes have a reviewable PR and are left In Review with explicit remaining owner acceptance.
- [ ] #6 Owner reviews the Inbox placement and themed navigation badges in the review build before this task is marked Done.
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implementation underway on feat/tui-inbox-badges in /Users/bpc/Dev/.worktrees/switchbard-inbox. Inbox has no action source yet. Exact PR total is independent of the bounded historical list. Initial live rendered checks passed against switchbard (positive open count) and budget (zero), including theme hot reload, filtering, all pages, and cached refresh failure. Delivery and owner acceptance remain outstanding.

Bug found in independent review and reproduced through the real app/live GitHub E2E: legacy named themes omit attention_badge, causing unpainted counts. Repro log: /tmp/switchbard-inbox-evidence/legacy-theme-repro.log (expected orange background, got None). Fix uses existing Chip surface when new semantic token is omitted; explicit overrides remain authoritative.

Committed 05b18a13. Local install gate passed: 121 TUI tests, 19 opt-in ignored; live badge journey explicitly run against positive and zero repositories. Core PR-list tests 18 passed; core clippy passed. Live large-count repository and owner visual acceptance are not claimed. No-mistakes delivery run 01M21AD9CE4NSZE6P3GV964XSX is in progress; PR still owed, so status remains In Progress / ball:agent.

Delivery review found an additional bug: count-only refresh failure discarded the last known total even while fresh PR rows remained usable. No-mistakes is fixing this within the existing stale-observation behavior; stale value must retain its provenance and current error, never become a fresh or fabricated count. Run 01M21AD9CE4NSZE6P3GV964XSX.

Follow-up review of stale-count retention caught a zero-count regression: an old zero was displayed as a badge. The accepted rule remains hide zero, with only the stale/unknown indicator visible after a failed refresh. Delivery workflow is correcting that regression and removing a source-string-only test assertion. Owner also authorized merging the separate parent-picker work; that delivery is running independently.

Corrected pipeline head 65851f33 passed focused and authenticated live tests. Full mise preflight then ended with SIGKILL9 in GUI nav_ia_v2 while parent-picker full validation was also running. Holding retry until heavy checks can run serially; no assertion failure or successful full-preflight claim is inferred from the killed process.

Additional live scale evidence on the initial reviewed build: public rust-lang/rust observation returned 1376 open PRs while only 100 history rows were loaded; real-key badge journey passed at 40x8, 80x24, 120x40 and 180x50, filtering, legacy theme fallback and stale failure. Fixture path /tmp/switchbard-inbox-evidence/large-fixture-path, log live-large.log, OS-temp capture sbt-navigation-badges-1376.txt. Recheck on final delivered head after pipeline synchronization.

Full preflight passed serially and normal PR144 is open. Parent-picker PR143 merged at b5661281 with all 8 checks green. Inbox CI detected conflicts with updated main in app wiring and PR tests; active delivery pipeline is rebasing and will revalidate before claiming CI readiness or installing combined preview. Task stays In Progress / ball:agent while integration remains agent work.

Independent integration audit reproduced a merge-resolution regression on dcf507af: app/mod.rs omitted resume and task_parent modules, and restored old positional page resume instead of merged TASK173 named restart contract. Pipeline TUI compilation confirms missing parent-picker methods. Required repair: preserve merged parent-picker modules and named restart/error-reporting behavior, extend that format for Inbox, retain legacy resume compatibility, then run parent-picker/restart/Inbox/live badge journeys. No acceptance or installation claimed for the broken intermediate head.

Second integration audit on 68ac0b75 caught a malformed named restart payload (appended positional tuple breaks prior main reader) and unreachable pages3 legacy handling. Root also reproduced lib-test compile failure from missing inbox_page initializer; /tmp/switchbard-inbox-evidence/resume-integration-repro.log. Pipeline lint repair now explicitly fixes single-object named wire format, legacy Inbox restoration, relevant protocol/real-app regression tests, initializer and formatting before full preflight. Focused badge tests alone were insufficient, so no final ready claim is made.

Integration repair30f188ee passed independent re-audit: single named JSON restart output preserves old-reader compatibility; pages3 Inbox handled before unreadable guard; parent modules and task_row fixture retained. Root exact-head lib, Inbox, pages and task_parent tests passed (7+3+5+8 tests) in /tmp/switchbard-inbox-evidence/final-integration.log. First targeted invocation used nonexistent test names and was corrected without source changes. Full preflight still active; final PR push and CI not yet claimed. Legacy pages3 real native replay remains scheduled in review build.

PR144 is ready at30f188ee: no-mistakes checks-passed; GitHub six applicable checks passed, mission sidecar correctly skipped by scope. Integrated review installed via mise run tui-install from detached exact-head worktree switchbard-inbox-review, includes merged parent143; path /Users/bpc/.local/share/switchbard-builds/inbox/bin/sbt. Native80x24 PTY restored actual pages3=inbox legacy record directly into Inbox, displayed real open PR count1 on inactive PR heading, and switching through Tasks to theme berg then back showed orange fill/black numeral. No task writes in that native journey. Final large live journey passed at30f188ee; log live-large-final.log. Next actor owner: inspect navigation placement and theme badges, then approve/merge PR144. AC6 stays unchecked. Original local feature branch remains preserved at05b18a13 because guarded sync refused rebased divergence; all repairs are durably committed/pushed, review worktree is exact remote30f188ee.

Released unfinished by session codex-in: Engineering delivery and CI complete in PR144; owner visual review of placement/color remains AC6. Leave In Review / ball:me, not Done. Review binary includes merged parent picker.

Default-install follow-through encountered a fixture timing defect: pages.rs109 unconditionally expected Loading pull requests after background invalid-repo refresh had correctly completed to Unavailable. /tmp/switchbard-inbox-evidence/install-default.log. No production failure inferred; no blind retry. Default binary was not replaced because gate stopped before installation. Existing isolated30f188ee build passed its install gate and native legacy-Inbox/color journey; owner can review it. Timing fixture correction routed into stacked abstraction delivery before any later default installation.

Final large-count capture is sbt-navigation-badges-1378.txt in OS temp: Tasks40x8 shows [Tasks] PRs 1378 Inbox; source row history remains bounded. Review binary SHA256 ef5299efa56e90a26457fc7d4387c56a566b4228c3a64afa07f6616a3f2f9cc7. Default binary checksum remains7e358fee8d039743403e21bb17e0ccc852cb45be790a3491f3b754c7a186cfc5 after blocked installation. TASK199 tracks the test timing defect; any final default replacement must report a new successful gate and checksum.

Default sbt replacement now succeeded from integrated abstraction commit d418be17 (runtime source unchanged from CI-green b4ce4547; fixture-only followup). mise run tui-install passed170TUI tests,0failed,21opt-in ignored; log /tmp/switchbard-abstractions-evidence/install-default-final.log. SHA25682b143c1c8f9d7d0434cb91210f045f300c8a63bc5f3ace42637469f9afe48e0. Actual default-binary PTY showed all7ranked/live abstraction tasks, both open PRs with linked tasks, badge2, and blank Inbox viaTab. Root session exitednormally. PR145 finalfollowupCI remains pending; InboxPR144 itself remainsgreen. Owner can now review navigation directly in default sbt.

Delivery update: GitHub records PR145 merged into the Inbox branch and combined PR144 merged into main at 6d0fa5bf50ace83d27bf337a00fc277033761e73. Both final PR check sets passed six checks with one scoped skip. The combined tree matches the reviewed abstraction head; installed default sbt has identical runtime code (only evidence documentation differs). Remains In Review / Ball me: next action is owner review of badge styling and Inbox placement in the normal TUI. AC6 remains unchecked pending explicit visual acceptance. Collapsible pane and actionable Inbox content remain deferred.
<!-- SECTION:NOTES:END -->
