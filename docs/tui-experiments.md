# One live TUI and experiments

Owner-directed 2026-09-17, TASK-286. Build continuously, commit small increments, and let the owner review at their own pace in one running `sbt`. There is no second user-facing version to maintain.

## Session contract

- During this exploration session, do not write or run automated tests. Compile runnable increments and record bounded manual observations of the changed behavior. Do not start unrelated test or infrastructure repair work.
- Routine backend fixes preserving intended behavior integrate without owner acceptance. Verify the specific behavior and use operation-level performance measurements where relevant; speed alone is not correctness.
- Product behavior decisions remain the owner's. An experiment defaults off. Enable/disable is separate from Keep/Remove. Keep enables it and records acceptance; Remove disables it and preserves a removal request. Neither action marks the board task Done.
- Accepted experiments are already integrated code. Later cleanup removes the flag and superseded implementation, or makes a useful choice a permanent setting. Cleanup and removal are agent work, not claims that choosing a menu item has already deleted code.
- Existing install ancestry and build-before-replace rules remain. This exception does not silently disable push hooks or required remote checks.

## First slice

The installer prepares the next compiled executable at the existing path. A running TUI notices the replacement and offers Update now; it does not restart by itself. `:update` or Settings > Update now requests reexec through the existing view-resume path. A fresh launch opens the installed build. Availability refers to the installed candidate, not arbitrary unbuilt commits.

Settings > Experiments and `:experiments` expose the compiled catalog. The number to try is the number of unreviewed entries in the running build, never a count inferred from commits. The update notice separately says when another build is ready. First catalog entry: TASK-251, Quick task editing (`t e`). Several catalog entries can independently remain enabled as more are built.

Decisions persist globally across repositories and updates. Failed writes leave effective behavior unchanged. Stale windows cannot overwrite newer decisions. Unrecognized entries survive updates so rollback does not erase intent. Failed or unreadable decision storage stays visible and is preserved.

Decisions live in `~/.switchbard/experiments.json`; `SWITCHBARD_EXPERIMENTS_FILE` selects a separate file for isolated sessions, and an empty override refuses mutation. Reopening Experiments refreshes another window's decisions. A removal request is durable intent for the agent's next sweep, not an automatically dispatched job.

## State and stress matrix

| State | Expected behavior | Evidence path |
| --- | --- | --- |
| No replacement / replacement ready | No auto restart; update reports current / requests reexec | Manual PTY observation; source inspection |
| Report save, merge, kill, draft, menu active | Request waits safely or explains retained input; no lost work | Source audit; manual draft observation |
| Candidate exec fails | Keep current session usable; explain retry | Source audit; manual replacement observation |
| Empty catalog / one / many | Honest empty state; reusable scrolling picker | Existing picker composition; first-entry manual observation; many remains a gap until available |
| Off / enabled / kept / removal requested | Independent enabled and review state; explicit reversible actions | Manual menu and persisted state observation |
| Corrupt / inaccessible / stale storage | Preserve source; no effective mutation on failure; actionable error | Source audit; bounded manual file observation |
| Narrow / short / wide | Reuse native picker scrolling; notices retain action | Manual PTY resize observation |
| Repeated actions / restart / reopen | No accidental acceptance from typing; durable state; refresh other-window changes on menu entry | Manual interaction observation |
| External services / network offline | Experiments require none; installer retains its existing refusal semantics | Source inspection |
| Pointer / touch / web zoom / roles | N/A for this keyboard-driven local menu; terminal font and size remain user-controlled | N/A |

## Objective ledger

- Objective: continuous creation in one live TUI with owner-paced feature review.
- Authorized: local implementation, small commits, experiment workflow and first board feature; no automated tests. No broad backlog drain or product decisions inferred from silence.
- Initial state: user checkout clean on `feat/tui-public-alpha` at `80f63720`; installed and fetched main at `cfd26571`. Implementation isolated in `feat/tui-experiments` from that main.
- Sequencing: registry and durable decisions + requested-restart boundary; menu integration; first usable experiment; compilation and bounded manual observations; commit and handoff for explicit live update.
- Compilation: `mise exec -- cargo build -p switchbard-tui -p switchbard-task --bins` passed with workspace warning policy; no test commands or automated test code.
- Manual interaction: isolated tmux server `sbt-experiment-review`, separate `SWITCHBARD_DATABASE` and decision file under `/tmp/sbt-experiments-review.Gp7Uue`. Enable persisted; `t e` opened focused task details; Keep remained enabled with review=kept; Remove persisted disabled/removal_requested. A replaced executable advertised availability without restarting; explicit update against a non-executable replacement reported failure and kept the session usable; a valid replacement then reexeced successfully. Native terminal text was observed at 120x32 and 52x14. Captures are in that temporary directory; these are terminal readbacks, not screenshots or owner approval.
- Independent source review: no verified persistence, stale-write, flag-gating, or lost-input defect. Fixed the two findings: missing command completion and stale per-crate restart instructions. Restart now uses the path captured before replacement, avoiding Linux's deleted-inode path on reexec; Linux execution is not verified here.
- Existing fixture notices: user configuration references a custom `place` column absent from the isolated board, and the scratch directory has no GitHub repository. Neither is a regression claim or an unrelated repair task.
- Gaps: pending-write/draft protection, corrupt storage, and competing-window conflicts were source-reviewed but not manually exercised; multiple catalog entries have not been exercised. The existing resume format restores list configuration and selection, but an open detail pane closes on restart. No claim of live installation, owner acceptance, complete visual coverage, remote CI, push, or merge.
