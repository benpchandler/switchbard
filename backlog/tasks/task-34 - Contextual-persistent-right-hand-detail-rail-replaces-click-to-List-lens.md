---
id: TASK-34
title: Contextual persistent right-hand detail rail (replaces click-to-List-lens)
status: Done
assignee: []
created_date: '2026-08-05 17:58'
updated_date: '2026-08-05 17:58'
labels:
  - backlog
  - ux
  - gui
dependencies: []
priority: high
ordinal: 34000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Owner: replace 'click a task -> jump to List lens' with a persistent right-hand detail rail, contextual to selection, working from every lens (Board, List, Digest, Milestones, search results). No selection -> quiet empty state. Also relocates 'Tracked repos' to a Servers-local left-side panel to free the right edge for the rail, with repo add/remove additionally reachable from a new Settings window from any view.
<!-- SECTION:DESCRIPTION:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
New ui/backlog/rail.rs: render_detail_rail is a SidePanel::right (resizable, 320-720px) rendered before the CentralPanel in ui/backlog/mod.rs's render(), reusing detail::render_task_detail unchanged — same content, different placement, so every existing mutation (Save, checklist toggles, notes, archive, dispatch) keeps working through the same shared Pending/apply_pending. Removed List's own embedded left-list+right-detail split (render_task_workspace is now just the task list at full width) since the rail is the one place detail renders now. Removed the lens=BacklogLens::List side effect from board.rs's card click, digest.rs's card click (kept the 'View all' button's lens switch, unrelated), and search.rs's result click -- clicking now only sets selected_task, since the rail shows it regardless of lens; milestones.rs already worked this way. New ui/settings.rs: a Settings window (opened via a new top-bar '⚙ Settings' button) with repo add/remove, reusing HiveApp::open_repo_picker and the existing confirm_remove_repo flow -- reachable from any view now that Tracked repos itself only renders in Servers view. sidebar.rs relocated from SidePanel::right to SidePanel::left (collapse/expand arrow glyphs mirrored accordingly), gated in app.rs's render_ui to ViewTab::Servers only; render_remove_confirmation extracted to render unconditionally (pub(crate)) since repo removal must work from Settings on any view too, not just when the Servers-only panel itself is visible. Updated several pre-existing tests whose fixtures now have TWO detail-showing surfaces at once (a single-task fixture's auto-selected task renders its title in both the source card/row AND the rail's heading) -- fixed by using two-task fixtures with an explicit Task-key sort so the interaction target isn't the auto-selected one, or by switching an exactly-one query to query_all. Also fixed 'Save' button index assumptions in three tests (rail's field-editor/Dependencies Save now render before, not after, the saved-views bar's Save, since the rail is a SidePanel shown before the CentralPanel). New coverage: 4 tests proving the rail actually updates to show the clicked task's detail from Board/List/Digest plus the empty-state case, 2 sidebar-relocation tests (Servers-only, left-side, mirrored arrows), and 3 Settings-window tests (opens with the repo list, Remove wires to the same confirm modal, reachable from Backlog view). Added a Settings-window fixture to legibility_audit.rs (both themes) since no other fixture ever sets settings_open.
<!-- SECTION:NOTES:END -->
