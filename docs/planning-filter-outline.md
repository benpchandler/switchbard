# Planning filters and project outlines

## Objective and boundaries

Owner screenshot shows a saved Budget view with columns id/status/priority/title/ball, 62 tasks under a single Planned heading despite `outline:project`. Planning must be discoverable through `f`, and choosing Project must actually expose project structure. Preserve task planning values, canonical planned order, saved layouts and existing paint rules. Work only in the isolated feat/tui-planning-outline worktree; the primary checkout was clean. The guarded installation ef50e871 completed without dropped commits. Real repository use is read-only except this repository's authorized tracker entries. No task migration or Budget content edits are needed.

## State and stress matrix

| State | Required evidence |
| --- | --- |
| Saved view omits Planning | `f` keyboard journey exposes planning without adding columns |
| Planning shown and hidden | Filter picker counts/values and selection remain coherent |
| Planned, Considering, filtered and empty | Real-app buffer and filter result assertions |
| Planned ordered section plus project outline | Real-app project headings with canonical order unchanged |
| Mixed plans, missing project and legacy ranking | Regression coverage and stable task identity |
| Group on/off, repeated navigation and cancellation | Real key events, no task mutation |
| Narrow viewport and long headings | Render clipping/no panic, selected task remains visible |
| Save/reload | Existing saved-view coverage plus focused assertions |
| Remote loading/permissions | N/A: local formatting/filtering introduces no remote authority |
| Pointer/touch | No new pointer or touch interaction; existing keyboard controls own this change |

## Evidence

The human screenshot is the reported failure; the isolated deterministic reproduction and installed readback are recorded below.

The filter access check found no missing field: `f` lists visible columns first, then hidden fields. With the reported saved layout, Planning is in the hidden section. The direct name path is `f` then `pl`, followed by `p` for Planned or `c` for Considering (the non-planned state). No filter-menu implementation change is justified; regressions protect hidden/shown access and narrow scrolling/cancellation.

Live installed-build reproduction in this repository confirmed `outline:project` alongside an ungrouped Planned61 section (`docs/evidence/planning-filter-outline/before.txt`). The fix keeps Planned at the top, applies the requested outline inside it and inside remaining tasks, preserves canonical order within each leaf, and returns to exact planned order with outline off. This is a read-only presentation change, not a reorder or migration of task data.

Real-data dogfood now shows project headings nested beneath Planned, including no-project tasks, with Project still selected in the context line. `docs/evidence/planning-filter-outline/after.ansi` records the real terminal output at 140x32; `after.png` is its Menlo rasterization, not a native OS screenshot. Visual Review target after-be93fdb62e03 owns revision revision-a95c912f6b593a165f98. Independent source review found no blocker in task partitioning, heading depth/raw paint keys, legacy mode or flat-order restoration.

Focused verification passed: 3 filter-access E2Es, 26 grouping/planning/rank tests, and a fresh 3-test outline rerun covering final review assertions. Coverage includes wrappers/depth, exact task identity multiset, selection through project/off, raw heading paint, mixed planning/project outlines, narrow navigation and byte-identical task/ranking snapshots. No filter production code changed. Formatting, Clippy and the full TUI sweep passed (422 passed, 22 existing ignored); the guarded temporary installation of ef50e871 updated both binaries without dropped commits.

Publication preflight exposed an unrelated allocator test whose spawned workers inherited no thread-local database override. Each worker now scopes a nonexistent database path inside its temporary fixture, preserving legacy allocation without opening host storage. All four concurrent writers and unique-ID/file assertions remain, with no retries or production timeout changes.
