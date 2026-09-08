# TUI menu picker consolidation

Objective: replace menu choices packed into status/footer prose with selectable shared picker rows. Preserve task creation (`t n`), task ranking and actions, saved view shortcuts, existing column/paint/settings controls and the PR-enabled installed experience. TASK-178 tracks the work; full task editing/archive features are outside this consolidation.

Baseline: new worktree feat/tui-menu-pickers clean at 11180ac, matching installed Cargo provenance. Primary checkout has unrelated task/report edits and untracked bug reports; preserve them. Build gates use isolated Cargo target to avoid cross-worktree artifact replacement previously observed.

## Inventory and decisions

- Task chord: new, rank positions, ball, done, append, drop, pin, goals move into shared picker rows.
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

Completed inventory and source consolidation. Isolated `mise run tui-install` passed formatting, warning-free clippy, 112 tests and rustdoc; 18 opt-in/evidence tests were skipped in that run. The evidence exporter was subsequently run explicitly and passed. Independent review reported no remaining verified correctness findings. Installed Cargo provenance points to this PR-enabled branch. Human visual approval is not presumed. Root retained acceptance and installation ownership; source worker owned only TUI source and scoped guidance.


Compatibility decisions: legacy column-name typeahead continues to toggle a unique matching column immediately, preserving existing shortcuts. New task/view/action-row text filtering requires explicit selection. View errors remain in the open picker for correction or Esc; the message is shortened because legal destinations are visible as rows. Existing input prompts/results are not duplicated as menu hints.


## Render evidence and explicit limits

The `menu_render_evidence` harness exports the real ratatui buffer (symbols and cell colors) at 100x20 and 40x8. PNGs in `docs/verification/tui-menu-pickers/` rasterize those buffers with Menlo; these are fixture renders, not screenshots of the owner desktop. Behavior tests also cover 160x30. Task, Views, Save View, Columns, Settings and the narrow task picker are rendered; a clipped picker exposes navigation and selected/total position on its border. Existing PR tests cover unchanged loading, error and guarded merge states. Task identity survives external reorder; disappearing targets cancel their menu rather than retargeting.

The task picker displays up to 10000 rank choices; numeric rank entry uses the actual queue length and preserves append-past-end behavior independently of that display bound. Existing text-editor and color/navigation hints remain inside their surfaces. No new network action was exercised; 17 existing opt-in tests remain unrun.

Visual Review owns a copy under repository `/Users/bpc/Dev/switchbard`, target `tasks-1c2c7d5054ea`, revision `revision-8ee0025f23b56edd9366`. Capture ownership metadata names the primary checkout; the actual rendered source is this feature branch, based at 11180ac. No annotations were open at closeout. Review URL: http://127.0.0.1:50434/?token=97998c12a3d9fe500a49d36a027500ba1b438253599a83655a1d09af9b2f7a23
