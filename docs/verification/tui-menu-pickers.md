# TUI menu picker consolidation

Objective: replace menu choices packed into status/footer prose with selectable shared picker rows. Preserve task creation (`t n`), task ranking and actions, saved view shortcuts, existing column/paint/settings controls and the PR-enabled installed experience. TASK-178 tracks the work; full task editing/archive features are outside this consolidation.

Baseline: recreated clean at6666334 for owner review follow-ups after the initial consolidation; original new worktree feat/tui-menu-pickers clean at 11180ac, matching installed Cargo provenance. Primary checkout has unrelated task/report edits and untracked bug reports; preserve them. Build gates use isolated Cargo target to avoid cross-worktree artifact replacement previously observed.

## Inventory and decisions

- Task chord: new, ball, status, project, Top list membership/ranks and goals move into shared picker rows.
- View chord and save/global destination chords: named slots and actions become shared picker rows.
- Existing Columns and Settings pickers: actions previously hidden in hints become selectable rows.
- Paint controls: inventory all hidden actions, expose operations as rows where applicable, keep movement/back/input hints concise.
- Outcomes, validation/errors, text-entry instructions and ordinary keyboard navigation are not menus and remain concise feedback.

## State and stress matrix

| Dimension | Evidence / expected behavior |
| --- | --- |
| Default, active, empty task list | Task picker renders actions; New remains reachable with no selection |
| Dirty draft / cancel / repeated actions | Existing creation E2Es plus menu escape and repeat-key cases; no accidental write |
| Saving / failure / retry | Existing view persistence E2Es; failure status remains visible after picker closes |
| Stale, conflict | Core remains authority for mutations; refreshed datasets use existing snapshots; no new async writes |
| Loading / unavailable PRs | Page and PR suites preserve independent page state and merge safeguards |
| Zero/one/many values and slots | Real-key tests cover empty tasks, maximum 9 view slots, rank positions above9 |
| Long / Unicode / unbroken text | Picker content uses existing bounded viewport and width-aware rendering; fixture labels exercise clipping |
| Narrow/current/wide, short viewport | Real ratatui fixture renders at40x8,100x20,160x30; focused rows stay visible |
| Keyboard / focus | Arrow and keyed selection, Enter, Esc, typeahead; same shared picker handler |
| Transitions | Nested menus, back/cancel, PR/tasks view separation, tick while picker active |
| Performance | Bounded rank/menu lists; no new network or process I/O in rendering |
| Historical/read-only/access-removed roles | N/A no role model; filesystem errors use ordinary failure feedback |
| Offline/reconnect | No new network behavior; existing PR dependency states remain unchanged |
| Pointer/touch/browser zoom | N/A keyboard terminal; resize covers terminal text scaling |

## Evidence ledger

Completed inventory and source consolidation. Isolated `mise run tui-install` passed formatting, warning-free clippy, 117 tests and rustdoc; 18 opt-in/evidence tests were skipped in that run. The evidence exporter was subsequently run explicitly and passed. Independent review reported no remaining verified correctness findings. Installed Cargo provenance points to this PR-enabled branch. Human visual approval is not presumed. Root retained acceptance and installation ownership; source worker owned only TUI source and scoped guidance.


Compatibility decisions: legacy column-name typeahead continues to toggle a unique matching column immediately, preserving existing shortcuts. New task/view/action-row text filtering requires explicit selection. View errors remain in the open picker for correction or Esc; the message is shortened because legal destinations are visible as rows. Existing input prompts/results are not duplicated as menu hints.


## Render evidence and explicit limits

The `menu_render_evidence` harness exports the real ratatui buffer (symbols and cell colors) at 100x20 and 40x8. PNGs in `docs/verification/tui-menu-pickers/` rasterize those buffers with Menlo; these are fixture renders, not screenshots of the owner desktop. Behavior tests also cover 160x30. Task, Status, Project, Top list, Views, Save View, Columns, Settings and the narrow task picker are rendered; a clipped picker exposes navigation and selected/total position on its border. Existing PR tests cover unchanged loading, error and guarded merge states. Task identity survives external reorder; disappearing targets cancel their menu rather than retargeting.

The task picker displays up to 10000 rank choices; numeric rank entry uses the actual queue length and preserves append-past-end behavior independently of that display bound. Existing text-editor and color/navigation hints remain inside their surfaces. No new network action was exercised; 17 existing opt-in tests remain unrun.

Visual Review owns a copy under repository `/Users/bpc/Dev/switchbard`, target `tasks-1c2c7d5054ea`, revision `revision-09bfc982bcd3e2f36db7`. Capture ownership metadata names the primary checkout; the actual rendered source is this feature branch, based at 11180ac. All five owner annotations are resolved against revised captures; `visual-review check` reports no unresolved annotations. Review URL: http://127.0.0.1:51378/?token=d32bb7231276675780a22731d112c8fe2d45e6662e405102837d5de6d63ea471


## Owner review: generic task status

Annotation annotation-a1ff9ce162c4441eb6697f76fe5bb5b8 requests replacing Mark Done with s Status and a subsequent picker. Task t s now offers the repo configured assignable statuses, marks the current status, and writes through the native editor only after explicit selection. The task ID is bound across reload and cancellation. Status tests cover Done/no-op repetition, custom configured status, typeahead without a write, Esc, and disappearing task. The former t d shortcut is replaced by t s plus a status choice as expressly requested. Done and custom configured statuses have passing persistence, cancellation and stale-target coverage.


## Owner review: navigation, projects and Top list

Additional annotations request arrow/Vim back navigation, project links, and clearer Top-list control separation. New navigation is Left/h parent, Right/l choose, Up/Down/j/k select; merge authorization remains restricted to its guarded explicit gestures. Task p assigns a project (or Unassigned). Task r contains rank/add/remove membership; Views p controls whether the top-list section is shown. Hiding that section does not alter ranked membership. Existing numeric ranking and append shortcuts remain where non-conflicting. The parent stack is bounded to 16, clears on Esc or stale cancellation, and retains guarded merge input. Legacy filter-value typeahead takes priority when h/l starts a matching label; Left/Right remain navigation keys.

Final gate: isolated `mise run tui-install` passed fmt, clippy, 117 tests and rustdoc, then replaced `/Users/bpc/.cargo/bin/sbt` from this PR-enabled worktree. The explicit render exporter also passed. Full-suite regressions in legacy filter typeahead and paint back-navigation were repaired before this gate. Nine fixture renders are retained. No push, merge, or remote mutation was performed.
