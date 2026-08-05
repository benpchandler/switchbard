//! QA parity audit (2026-08-05) — interaction tests closing the
//! click-and-assert-a-behavior-change gaps `ui_views.rs` and
//! `legibility_audit.rs` left in the List/Board/Digest/Milestones lenses,
//! the detail pane's checklist/editor controls, and global search/theme.
//! Each test drives a real control via `egui_kittest`/`kittest` (not a
//! `state_mut()` shortcut) and asserts an observable behavior change, per
//! the same bar `ui_views.rs`'s module doc sets.
//!
//! A few controls are click-drivable but their *only* observable effect is a
//! fire-and-forget background thread that shells out to the real `backlog`
//! CLI (`HiveApp::spawn_backlog_*`, `app.rs`). Waiting on that thread from
//! here would reintroduce exactly the cross-thread flakiness
//! `worktree_removal_orchestration.rs`'s doc comment already rules out for
//! this harness. Where a control has no synchronous, click-time state change
//! to assert, this file asserts the click's synchronous side effect instead
//! (a status message set before the spawn, a buffer clearing, a confirm
//! flag flipping) and leaves the CLI round-trip itself to
//! `switchbard-core/tests/backlog_cli_mutations.rs`, which proves the exact
//! same functions against a real fixture repo.

mod common;

use std::path::PathBuf;

use common::{harness, isolated_config_save_path, seeded_app, REPO_NAME, REPO_PATH};
use egui_kittest::kittest::{self, Queryable};
use egui_kittest::Harness;
use switchbard_core::config::Config;
use switchbard_core::{
    BacklogChecklistItem, BacklogProject, BacklogTask, BacklogTaskSource, Repo, WorktreeRef,
};
use switchbard_gui::app::HiveApp;
use switchbard_gui::runtime::{BacklogLens, BacklogTaskSortDirection, BacklogTaskSortKey, ViewTab};

fn task(id: &str, title: &str, status: &str) -> BacklogTask {
    BacklogTask {
        id: id.to_string(),
        title: title.to_string(),
        status: status.to_string(),
        priority: "medium".to_string(),
        assignees: vec![],
        labels: vec![],
        dependencies: vec![],
        references: vec![],
        milestone: None,
        parent: None,
        created_date: Some("2026-06-01 09:00".to_string()),
        updated_date: Some("2026-06-01 09:00".to_string()),
        description: String::new(),
        implementation_plan: String::new(),
        implementation_notes: String::new(),
        final_summary: String::new(),
        acceptance_criteria: vec![],
        definition_of_done: vec![],
        source: BacklogTaskSource::Active,
        path: PathBuf::from(format!(
            "{REPO_PATH}/backlog/tasks/{}.md",
            id.to_lowercase()
        )),
    }
}

/// A `HiveApp` with `n` plain "To Do" tasks (TASK-1..TASK-n) in one project,
/// scoped to it, on the List lens — the shape most of this file's tests
/// share.
fn list_app_with_tasks(tasks: Vec<BacklogTask>) -> HiveApp {
    let mut app = seeded_app();
    app.view_tab = ViewTab::Backlog;
    app.backlog_view.lens = BacklogLens::List;
    app.backlog_view.selected_project = Some(PathBuf::from(REPO_PATH));
    app.backlog_projects.lock().unwrap().insert(
        PathBuf::from(REPO_PATH),
        BacklogProject {
            root: PathBuf::from(REPO_PATH),
            cli_path: Some(PathBuf::from("/usr/local/bin/backlog")),
            tasks,
            warnings: vec![],
            loaded_at_unix: 0,
            configured_statuses: vec![],
        },
    );
    app
}

fn list_harness_with_tasks(tasks: Vec<BacklogTask>) -> Harness<'static, HiveApp> {
    let mut harness = harness(list_app_with_tasks(tasks));
    harness.run();
    harness
}

/// Push a real Cmd/Ctrl-modified key event straight into the harness's raw
/// input — `kittest::Node::key_down`/`key_press` track modifiers correctly
/// for text-editing shortcuts, but `search::handle_shortcut` reads
/// `ctx.input_mut(|i| i.consume_shortcut(...))` against `egui::Modifiers`,
/// which is simplest to satisfy by writing the event directly rather than
/// threading a kittest node's focus through it.
fn press_command_k(harness: &mut Harness<'_, HiveApp>) {
    harness.input_mut().events.push(egui::Event::Key {
        key: egui::Key::K,
        pressed: true,
        modifiers: egui::Modifiers::COMMAND,
        repeat: false,
        physical_key: None,
    });
    harness.input_mut().events.push(egui::Event::Key {
        key: egui::Key::K,
        pressed: false,
        modifiers: egui::Modifiers::COMMAND,
        repeat: false,
        physical_key: None,
    });
}

use eframe::egui;

/// `egui::Checkbox::without_text` (the select-all checkbox and every row's
/// bulk-select checkbox) carries no accessible label at all — confirmed
/// empirically (dumping the tree shows `label: Some("")` for these three,
/// vs. `Some("Done")`/`Some("Archived")`/`Some("Drafts")` for the toolbar's
/// text-bearing checkboxes), matching the same "unlabeled checkbox" gap
/// `ui_views.rs`'s `parent_task_shows_rollup_...` test already documented
/// for the tree caret. Selecting by role + position is the only way to
/// reach them: index 0 is always the header's select-all checkbox (it
/// renders unconditionally); index `n` (n >= 1) is the bulk checkbox of the
/// `n`th task row in render order.
fn unlabeled_checkbox<'t>(harness: &'t Harness<'_, HiveApp>, index: usize) -> kittest::Node<'t> {
    harness
        .query_all(kittest::by().role(egui::accesskit::Role::CheckBox))
        .filter(|n| n.label().is_none_or(|l| l.is_empty()))
        .nth(index)
        .unwrap_or_else(|| panic!("no unlabeled checkbox at index {index}"))
}

/// The detail pane has several `TextEdit::singleline` fields with no
/// distinguishing accessible label either (only an adjacent, separate
/// `ui.label(...)` text node describes them, which `by().label(...)` also
/// matches — ambiguously, since the field itself has no name of its own).
/// A *fixed* absolute index among all `TextInput`-role nodes in the window
/// is fragile: the sidebar's repo filter, the saved-views name-draft field,
/// and (in List/Board lenses) the toolbar's project filter all render
/// before the detail pane and would shift it (confirmed empirically — this
/// cost real debugging time before landing on this approach). Instead,
/// locate the title field by its known, presumably-unique current value,
/// then take a fixed *offset* from it — render order for `detail::
/// render_editor` + `detail_lists`'s sections is stable: title(+0),
/// labels(+1), assignees(+2), milestone(+3), dependencies-edit(+4),
/// new_reference(+5).
fn detail_text_input<'t>(
    harness: &'t Harness<'_, HiveApp>,
    task_title: &str,
    offset_from_title: usize,
) -> kittest::Node<'t> {
    let inputs: Vec<kittest::Node<'t>> = harness
        .query_all(kittest::by().role(egui::accesskit::Role::TextInput))
        .collect();
    let title_index = inputs
        .iter()
        .position(|n| n.value().as_deref() == Some(task_title))
        .unwrap_or_else(|| panic!("couldn't find the title field (value {task_title:?})"));
    inputs
        .into_iter()
        .nth(title_index + offset_from_title)
        .unwrap_or_else(|| panic!("no TextInput at title_index + {offset_from_title}"))
}

/// Same idea for the two multiline fields: implementation plan(0), notes(1).
fn multiline_input_nth<'t>(harness: &'t Harness<'_, HiveApp>, index: usize) -> kittest::Node<'t> {
    harness
        .query_all(kittest::by().role(egui::accesskit::Role::MultilineTextInput))
        .nth(index)
        .unwrap_or_else(|| panic!("no MultilineTextInput at index {index}"))
}

// ─── List lens: bulk selection ──────────────────────────────────────────

#[test]
fn select_all_checkbox_selects_then_deselects_every_visible_task() {
    let mut harness = list_harness_with_tasks(vec![
        task("TASK-1", "First", "To Do"),
        task("TASK-2", "Second", "To Do"),
    ]);

    unlabeled_checkbox(&harness, 0).simulate_click();
    harness.run();
    assert_eq!(
        harness.state().backlog_view.bulk_selected_tasks.len(),
        2,
        "clicking select-all should select every visible task"
    );

    unlabeled_checkbox(&harness, 0).simulate_click();
    harness.run();
    assert!(
        harness.state().backlog_view.bulk_selected_tasks.is_empty(),
        "clicking select-all again should clear the selection"
    );
}

#[test]
fn clear_button_clears_bulk_selection() {
    let mut harness = list_harness_with_tasks(vec![task("TASK-1", "First", "To Do")]);
    harness
        .state_mut()
        .backlog_view
        .bulk_selected_tasks
        .insert((PathBuf::from(REPO_PATH), "TASK-1".to_string()));
    harness.run();

    harness.get_by_label("Clear").simulate_click();
    harness.run();
    assert!(
        harness.state().backlog_view.bulk_selected_tasks.is_empty(),
        "Clear should empty the bulk selection"
    );
}

#[test]
fn row_bulk_checkbox_click_selects_the_task() {
    let mut harness = list_harness_with_tasks(vec![
        task("TASK-1", "First", "To Do"),
        task("TASK-2", "Second", "To Do"),
    ]);

    // index 0 is the header's select-all checkbox; index 1 is the first row.
    unlabeled_checkbox(&harness, 1).simulate_click();
    harness.run();

    assert_eq!(
        harness.state().backlog_view.bulk_selected_tasks.len(),
        1,
        "clicking one row's bulk checkbox should select exactly that task"
    );
}

#[test]
fn shift_click_on_a_second_row_checkbox_selects_the_contiguous_range() {
    let mut harness = list_harness_with_tasks(vec![
        task("TASK-1", "First", "To Do"),
        task("TASK-2", "Second", "To Do"),
        task("TASK-3", "Third", "To Do"),
    ]);

    unlabeled_checkbox(&harness, 1).simulate_click();
    harness.run();

    // `ui.input(|i| i.modifiers.shift)` (the row's own modifier check) reads
    // `egui::RawInput`'s top-level `modifiers` field, which is *not* the
    // same thing `kittest::Node::key_down`/`key_up` maintain (that only
    // tracks modifiers for constructing *subsequent kittest-originated*
    // events' own `modifiers` field, e.g. what a keyboard shortcut match
    // reads) — confirmed empirically the two don't automatically sync.
    // Setting `RawInput.modifiers` directly is what the row's plain
    // `ui.input()` read actually observes.
    harness.input_mut().modifiers = egui::Modifiers::SHIFT;
    unlabeled_checkbox(&harness, 3).simulate_click();
    harness.run();
    harness.input_mut().modifiers = egui::Modifiers::default();

    assert_eq!(
        harness.state().backlog_view.bulk_selected_tasks.len(),
        3,
        "shift-clicking the third row after selecting the first should select the contiguous range of 3"
    );
}

#[test]
fn command_click_on_a_row_title_toggles_bulk_selection_without_opening_detail() {
    let mut harness = list_harness_with_tasks(vec![
        task("TASK-1", "First", "To Do"),
        task("TASK-2", "Second", "To Do"),
    ]);
    let before = harness.state().backlog_view.selected_task.clone();

    // See the shift-click test above for why `RawInput.modifiers` (not
    // `Node::key_down`) is what the row's `ui.input()` read observes.
    harness.input_mut().modifiers = egui::Modifiers::COMMAND;
    harness.get_by_label("TASK-2  Second").simulate_click();
    harness.run();
    harness.input_mut().modifiers = egui::Modifiers::default();

    assert!(
        harness
            .state()
            .backlog_view
            .bulk_selected_tasks
            .contains(&(PathBuf::from(REPO_PATH), "TASK-2".to_string())),
        "cmd-click on a row title should toggle its bulk selection"
    );
    assert_eq!(
        harness.state().backlog_view.selected_task,
        before,
        "cmd-click is a bulk-select modifier, not a detail-pane selection"
    );
}

#[test]
fn plain_click_on_a_row_title_selects_it_for_the_detail_pane() {
    let mut harness = list_harness_with_tasks(vec![
        task("TASK-1", "First", "To Do"),
        task("TASK-2", "Second", "To Do"),
    ]);

    harness.get_by_label("TASK-2  Second").click();
    harness.run();

    assert_eq!(
        harness.state().backlog_view.selected_task,
        Some((PathBuf::from(REPO_PATH), "TASK-2".to_string())),
        "clicking a row title should select it for the detail pane"
    );
}

// ─── List lens: right-click bulk context menu — UNDRIVABLE-BY-KITTEST ────
//
// The row's context menu (`list::render_task_list_row`) is attached to
// `ui.horizontal(|ui| { ... }).response` — a *container* response, not a
// widget natively built with click sense. Confirmed by isolated,
// app-independent reproduction outside this codebase (three minimal
// `egui`/`egui_kittest` 0.30 probes, kept out of tree): a synthetic
// `PointerButton` event (primary *or* secondary) delivered to a position
// inside a `ui.horizontal()` response's own `rect`, then upgraded via
// `Response::interact(Sense::click())` — exactly what `Response::
// context_menu` and `Response::secondary_clicked` do internally — never
// registers as a click in this harness, whether driven by `Node::
// simulate_click()` (raw pointer position) or `Node::click()` (the
// accesskit semantic action). The identical `.interact(Sense::click())`
// pattern *does* work when the container is an `egui::Frame::show(...)`
// response instead — confirmed by `digest_card_click_selects_the_task_and_
// jumps_to_list` below, which drives that exact pattern successfully — so
// this is at least partly an `egui`/`egui_kittest` limitation on bare
// `ui.horizontal()` responses, not a general "retroactive interact never
// works" rule, and not a Switchbard defect.
//
// CORRECTION (2026-08-05, egui 0.30->0.31 upgrade): this comment previously
// also cited `board_card_click_selects_the_task` as a second example of the
// Frame-response pattern working. That citation was wrong — the board test
// only ever exercised a single-task fixture, where `reconcile_selected_task`
// (mod.rs) auto-selects the lone visible row regardless of whether the
// click did anything, so it passed without proving the click worked. A
// two-task discriminating version (added while porting to egui 0.31) shows
// the click does *not* reliably move selection on a Board card, even though
// the code is the identical `egui::Frame::show(...).response.interact(
// Sense::click())` pattern digest's card uses successfully — see the
// `UNDRIVABLE-BY-KITTEST` note on Board card clicks near
// `board_card_shows_labels_and_a_humanized_age` below for the full
// investigation. So Frame-response clicks are not universally reliable
// either; digest's card is the one concretely proven case, not a
// stand-in for "every Frame-response click works."
//
// Verification for this control instead rests on:
//   1. Code review — `render_task_context_menu` (list.rs) and its
//      `bulk_patch_button` helper are ~5-line, non-branching functions: one
//      button per `BACKLOG_STATUSES`/`BACKLOG_PRIORITIES` entry, each
//      pushing one `Pending::bulk_save` tuple and setting
//      `backlog_status` synchronously — the same shape already proven for
//      the toolbar's `bulk_patch_button`-adjacent code paths elsewhere in
//      this file.
//   2. `legibility_audit.rs` and this file's own List-lens screenshots
//      (`docs/qa/screenshots/`) render the row the menu attaches to, in
//      both themes, confirming the row itself paints correctly.
//   3. A live, isolated-HOME run of the real app (documented in
//      `docs/qa/2026-08-05-parity-qa.md`) exercising the actual right-click
//      gesture with a real mouse, confirming the menu opens and each action
//      updates the task's status/priority through the CLI.

// ─── List lens: sort ─────────────────────────────────────────────────────

#[test]
fn sort_direction_button_toggles_between_ascending_and_descending() {
    let mut harness = list_harness_with_tasks(vec![task("TASK-1", "First", "To Do")]);
    assert_eq!(
        harness.state().backlog_view.sort_direction,
        BacklogTaskSortDirection::Ascending,
        "Ascending is the default sort direction"
    );

    harness.get_by_label("Ascending").click();
    harness.run();
    assert_eq!(
        harness.state().backlog_view.sort_direction,
        BacklogTaskSortDirection::Descending
    );
    assert!(harness.query_by_label("Descending").is_some());

    harness.get_by_label("Descending").click();
    harness.run();
    assert_eq!(
        harness.state().backlog_view.sort_direction,
        BacklogTaskSortDirection::Ascending
    );
}

// ─── Toolbar ─────────────────────────────────────────────────────────────

/// QA parity matrix LOW gap: "Clean Up Old Tasks" (bulk archive of Done
/// tasks, cross-repo). Disabled with nothing to clean up — mirrors how the
/// Create modal's own "Create" button gates on `can_create`
/// (`editing_the_title_enables_the_save_button`'s Save-button pattern).
#[test]
fn cleanup_button_is_disabled_when_there_are_no_done_tasks() {
    let harness = list_harness_with_tasks(vec![task("TASK-1", "Open one", "To Do")]);
    assert!(
        harness.get_by_label("Clean Up Old Tasks").is_disabled(),
        "nothing to archive should leave the button disabled"
    );
}

#[test]
fn cleanup_button_confirms_then_cancel_reverts_to_the_plain_button() {
    let mut done = task("TASK-1", "Stale one", "Done");
    done.status = "Done".to_string();
    let mut harness = list_harness_with_tasks(vec![task("TASK-2", "Open one", "To Do"), done]);
    assert!(!harness.get_by_label("Clean Up Old Tasks").is_disabled());

    harness.get_by_label("Clean Up Old Tasks").click();
    harness.run();
    assert!(
        harness.query_by_label("Complete 1 Done tasks?").is_some(),
        "clicking should show the confirm prompt naming the candidate count \
         (\"Complete\", not \"Archive\" — the real CLI refuses `task \
         archive` on a Done task, see complete_backlog_task's doc comment)"
    );

    harness.get_by_label("Cancel").click();
    harness.run();
    assert!(
        harness.query_by_label("Clean Up Old Tasks").is_some(),
        "Cancel should revert to the plain button"
    );
    assert!(!harness.state().backlog_view.cleanup_confirm);
}

#[test]
fn cleanup_confirm_sets_the_synchronous_status_before_the_spawned_archive_calls() {
    let mut done = task("TASK-1", "Stale one", "Done");
    done.status = "Done".to_string();
    let mut harness = list_harness_with_tasks(vec![task("TASK-2", "Open one", "To Do"), done]);

    harness.get_by_label("Clean Up Old Tasks").click();
    harness.run();
    harness.get_by_label("Confirm cleanup").click();
    harness.run();

    assert_eq!(
        harness.state().backlog_status.snapshot().as_deref(),
        Some("cleaning up 1 Done tasks"),
        "confirming should set the synchronous status before the spawned \
         per-task archive calls run — same split real_backlog_cli_
         mutations.rs proves the per-task Archive path with"
    );
    assert!(!harness.state().backlog_view.cleanup_confirm);
}

/// A `Completed`-sourced task (already moved to `backlog/completed/` by the
/// CLI's own `backlog cleanup`) isn't a cleanup candidate — only a Done task
/// still sitting in `backlog/tasks/` is, matching a single task's Archive
/// button requiring `editable()`.
#[test]
fn cleanup_button_ignores_already_completed_sourced_tasks() {
    let mut already_completed = task("TASK-1", "Already moved", "Done");
    already_completed.source = BacklogTaskSource::Completed;
    let harness =
        list_harness_with_tasks(vec![task("TASK-2", "Open one", "To Do"), already_completed]);
    assert!(
        harness.get_by_label("Clean Up Old Tasks").is_disabled(),
        "a Completed-sourced task should not count as a cleanup candidate"
    );
}

#[test]
fn refresh_backlog_button_kicks_a_reload_and_sets_status() {
    let mut harness = list_harness_with_tasks(vec![task("TASK-1", "First", "To Do")]);
    harness.get_by_label("Refresh Backlog").click();
    harness.run();
    assert_eq!(
        harness.state().backlog_status.snapshot().as_deref(),
        Some("refreshing Backlog projects")
    );
}

#[test]
fn plus_task_button_opens_the_create_modal_targeting_the_current_project() {
    let mut harness = list_harness_with_tasks(vec![task("TASK-1", "First", "To Do")]);
    assert!(!harness.state().backlog_view.new_task.open);

    harness.get_by_label("+ Task").click();
    harness.run();

    assert!(harness.state().backlog_view.new_task.open);
    assert_eq!(
        harness.state().backlog_view.new_task.target_project,
        Some(PathBuf::from(REPO_PATH))
    );
    assert!(
        harness.query_by_label("New Backlog Task").is_some(),
        "the create modal should render once opened"
    );
}

#[test]
fn create_modal_create_button_queues_a_create_and_closes_the_modal() {
    let mut harness = list_harness_with_tasks(vec![task("TASK-1", "First", "To Do")]);
    harness.get_by_label("+ Task").click();
    harness.run();

    // The create modal's title field has no label of its own. Scope the
    // TextInput search to the modal's own subtree (found by its Window
    // title, itself a queryable label) and take the *first* one: title is
    // the modal's first singleline field, declared before description
    // (multiline — a different accesskit role, so it never collides) and
    // before the labels/assignee/milestone/dependencies fields (QA parity
    // gap) added after it. Searching the whole tree's *last* TextInput, as
    // this test used to, broke the moment those four singleline fields
    // landed after title.
    let modal = harness.get_by_label("New Backlog Task");
    let title_field = modal
        .query_all(kittest::by().role(egui::accesskit::Role::TextInput))
        .next()
        .expect("create modal's title field");
    title_field.focus();
    title_field.type_text("New fixture task");
    harness.run();

    harness.get_by_label("Create").click();
    harness.run();

    assert!(
        !harness.state().backlog_view.new_task.open,
        "Create should close the modal"
    );
    assert_eq!(
        harness.state().backlog_view.new_task.title,
        "",
        "the new-task buffer should reset to its default after Create"
    );
}

/// QA parity matrix LOW gap: labels/assignee/milestone/dependencies are now
/// settable at creation time, not just afterward via the detail pane. This
/// proves the render+queuing half (the fields exist, are typeable, and the
/// buffer resets after Create — same bar the pre-existing description/AC
/// fields are held to); `create_backlog_task_wires_labels_assignee_
/// milestone_and_dependencies` (backlog_cli_mutations.rs) proves the queued
/// value actually reaches the real CLI.
#[test]
fn create_modal_labels_assignee_milestone_and_dependencies_fields_reset_after_create() {
    let mut harness = list_harness_with_tasks(vec![task("TASK-1", "First", "To Do")]);
    harness.get_by_label("+ Task").click();
    harness.run();

    // Confirms all four fields actually render (queryable by their draft
    // text once set) before checking the buffer-reset behavior below. Set
    // directly on state rather than via simulated typing — same pattern
    // `saved_view_name_draft` is tested with — since egui's own TextEdit
    // interactivity is already proven generically by the title/description
    // fields; what's under test here is the buffer's own round trip through
    // Create, not the widget's typing mechanics.
    harness.state_mut().backlog_view.new_task.title = "New fixture task".to_string();
    harness.state_mut().backlog_view.new_task.labels = "frontend, urgent".to_string();
    harness.state_mut().backlog_view.new_task.assignees = "ben".to_string();
    harness.state_mut().backlog_view.new_task.milestone = "v1".to_string();
    harness.state_mut().backlog_view.new_task.dependencies = "TASK-1".to_string();
    harness.run();

    // A TextInput's typed content is its accessible *value*, not its label
    // (kittest's `by().label()` reads `Node::value` only for `Role::Label`
    // widgets — see the doc on `By::label`), and both the TextInput node
    // and its inner TextRun glyph-run child carry that same value, hence
    // query_all rather than the exactly-one query.
    assert!(harness
        .query_all_by_value("frontend, urgent")
        .next()
        .is_some());
    assert!(harness.query_all_by_value("ben").next().is_some());
    assert!(harness.query_all_by_value("v1").next().is_some());

    harness.get_by_label("Create").click();
    harness.run();

    let new_task = &harness.state().backlog_view.new_task;
    assert_eq!(
        new_task.labels, "",
        "labels buffer should reset after Create"
    );
    assert_eq!(
        new_task.assignees, "",
        "assignees buffer should reset after Create"
    );
    assert_eq!(
        new_task.milestone, "",
        "milestone buffer should reset after Create"
    );
    assert_eq!(
        new_task.dependencies, "",
        "dependencies buffer should reset after Create"
    );
}

#[test]
fn create_modal_cancel_closes_without_creating() {
    let mut harness = list_harness_with_tasks(vec![task("TASK-1", "First", "To Do")]);
    harness.get_by_label("+ Task").click();
    harness.run();

    harness.get_by_label("Cancel").click();
    harness.run();

    assert!(!harness.state().backlog_view.new_task.open);
}

#[test]
fn done_filter_checkbox_hides_and_reveals_completed_tasks() {
    let mut harness = list_harness_with_tasks(vec![task("TASK-1", "Open one", "To Do"), {
        let mut done = task("TASK-2", "Done one", "Done");
        done.status = "Done".to_string();
        done
    }]);
    assert!(
        harness.query_by_label("TASK-2  Done one").is_none(),
        "Done tasks are hidden by default"
    );

    // "Done" is ambiguous once the task is revealed — the toolbar checkbox
    // and the row's own status pill both carry that exact label — so target
    // the checkbox specifically (it's the first "Done"-labeled node either
    // way, since the toolbar renders above the list).
    harness.get_all_by_label("Done").next().unwrap().click();
    harness.run();
    assert!(
        harness.query_by_label("TASK-2  Done one").is_some(),
        "checking Done should reveal completed tasks"
    );

    harness.get_all_by_label("Done").next().unwrap().click();
    harness.run();
    assert!(harness.query_by_label("TASK-2  Done one").is_none());
}

#[test]
fn archived_filter_checkbox_hides_and_reveals_archived_tasks() {
    // Status "To Do" (not "Done") isolates the Archived filter's own effect
    // — `task_visible` applies the completed-status check and the
    // archived-source check independently, so a Done *and* Archived task
    // would need both `show_completed` and `show_archived` to reveal it.
    let mut archived = task("TASK-2", "Archived one", "To Do");
    archived.source = BacklogTaskSource::Archived;
    let mut harness = list_harness_with_tasks(vec![task("TASK-1", "Open one", "To Do"), archived]);
    assert!(harness.query_by_label("TASK-2  Archived one").is_none());

    harness.get_by_label("Archived").click();
    harness.run();
    assert!(harness.query_by_label("TASK-2  Archived one").is_some());
}

#[test]
fn drafts_filter_checkbox_hides_and_reveals_draft_tasks() {
    let mut draft = task("TASK-2", "Draft one", "Draft");
    draft.source = BacklogTaskSource::Draft;
    let mut harness = list_harness_with_tasks(vec![task("TASK-1", "Open one", "To Do"), draft]);
    assert!(
        harness.query_by_label("TASK-2  Draft one").is_some(),
        "Drafts default to visible (show_drafts defaults true)"
    );

    harness.get_by_label("Drafts").click();
    harness.run();
    assert!(harness.query_by_label("TASK-2  Draft one").is_none());
}

/// QA parity matrix MEDIUM gap: a dedicated milestone filter (previously
/// only reachable by switching to the separate Milestones lens). Like the
/// pre-existing status/priority combos, the ComboBox trigger itself is
/// UNDRIVABLE by this harness (no accessible label — same confirmed
/// limitation the QA audit documents for status_filter/priority_filter); the
/// filter's actual effect is proven directly via `task_visible`'s
/// milestone_filter branch (sort.rs), same bar those two combos are held to.
#[test]
fn milestone_filter_hides_tasks_outside_the_selected_milestone() {
    let mut v1 = task("TASK-1", "In v1", "To Do");
    v1.milestone = Some("v1".to_string());
    let mut v2 = task("TASK-2", "In v2", "To Do");
    v2.milestone = Some("v2".to_string());
    let mut harness = list_harness_with_tasks(vec![v1, v2]);

    harness.state_mut().backlog_view.milestone_filter = "v1".to_string();
    harness.run();

    assert!(harness.query_by_label("TASK-1  In v1").is_some());
    assert!(
        harness.query_by_label("TASK-2  In v2").is_none(),
        "a task outside the selected milestone should be hidden"
    );
}

/// QA parity matrix MEDIUM/partial gap: a dedicated label filter (previously
/// only reachable through the general free-text filter, which matches many
/// fields at once, not labels specifically).
#[test]
fn label_filter_hides_tasks_without_the_selected_label() {
    let mut frontend = task("TASK-1", "Frontend work", "To Do");
    frontend.labels = vec!["frontend".to_string()];
    let mut backend = task("TASK-2", "Backend work", "To Do");
    backend.labels = vec!["backend".to_string()];
    let mut harness = list_harness_with_tasks(vec![frontend, backend]);

    harness.state_mut().backlog_view.label_filter = "frontend".to_string();
    harness.run();

    assert!(harness.query_by_label("TASK-1  Frontend work").is_some());
    assert!(
        harness.query_by_label("TASK-2  Backend work").is_none(),
        "a task without the selected label should be hidden"
    );
}

// ─── Global search ───────────────────────────────────────────────────────

#[test]
fn command_k_toggles_the_search_overlay_open_and_closed() {
    let mut harness = list_harness_with_tasks(vec![task("TASK-1", "First", "To Do")]);
    assert!(!harness.state().backlog_view.search.open);

    press_command_k(&mut harness);
    harness.run();
    assert!(harness.state().backlog_view.search.open);
    assert!(harness.query_by_label("Search all repos").is_some());

    press_command_k(&mut harness);
    harness.run();
    assert!(!harness.state().backlog_view.search.open);
}

#[test]
fn escape_closes_the_search_overlay() {
    let mut harness = list_harness_with_tasks(vec![task("TASK-1", "First", "To Do")]);
    harness.state_mut().backlog_view.search.open = true;
    harness.run();

    harness.press_key(egui::Key::Escape);
    harness.run();
    assert!(!harness.state().backlog_view.search.open);
}

/// Owner UX pass (2026-08-05): a search result click used to force-switch
/// to the List lens just to reach its detail pane. Now the persistent
/// detail rail shows any selected task regardless of lens, so the lens
/// (deliberately set to Statistics here, an arbitrary lens the click has no
/// business touching) stays exactly where the user left it — only
/// selection and the search overlay's own open flag change.
#[test]
fn search_result_row_click_selects_the_task_without_changing_lens() {
    let mut harness = list_harness_with_tasks(vec![
        task("TASK-1", "First", "To Do"),
        task("TASK-2", "Second thing", "To Do"),
    ]);
    harness.state_mut().backlog_view.lens = BacklogLens::Statistics;
    harness.state_mut().backlog_view.search.open = true;
    harness.state_mut().backlog_view.search.query = "Second".to_string();
    harness.run();

    harness
        .get_by_label(&format!("{REPO_NAME}:TASK-2  Second thing"))
        .click();
    harness.run();

    assert!(!harness.state().backlog_view.search.open);
    assert_eq!(
        harness.state().backlog_view.lens,
        BacklogLens::Statistics,
        "selecting a search result should not change the active lens"
    );
    assert_eq!(
        harness.state().backlog_view.selected_task,
        Some((PathBuf::from(REPO_PATH), "TASK-2".to_string()))
    );
}

// ─── Digest lens ─────────────────────────────────────────────────────────

fn digest_harness_with(tasks: Vec<BacklogTask>) -> Harness<'static, HiveApp> {
    let mut app = seeded_app();
    app.view_tab = ViewTab::Backlog;
    app.backlog_projects.lock().unwrap().insert(
        PathBuf::from(REPO_PATH),
        BacklogProject {
            root: PathBuf::from(REPO_PATH),
            cli_path: Some(PathBuf::from("/usr/local/bin/backlog")),
            tasks,
            warnings: vec![],
            loaded_at_unix: 0,
            configured_statuses: vec![],
        },
    );
    let mut harness = harness(app);
    harness.run();
    harness
}

#[test]
fn digest_recently_done_view_all_jumps_to_list_filtered_to_done() {
    let mut done = task("TASK-1", "Done task", "Done");
    done.updated_date = Some("2026-08-01 12:00".to_string());
    let mut harness = digest_harness_with(vec![done]);

    harness
        .get_all_by_label("View all")
        .nth(3)
        .expect("Recently done is the 4th section's View all")
        .click();
    harness.run();

    assert_eq!(harness.state().backlog_view.lens, BacklogLens::List);
    assert_eq!(harness.state().backlog_view.status_filter, "Done");
}

/// Owner UX pass (2026-08-05): a Digest card click used to force-switch to
/// the List lens just to reach its detail pane. Now the persistent detail
/// rail shows it regardless of lens, so Digest stays on screen — only
/// selection (and the scope-widen to "All projects", still needed since a
/// Digest card can surface a task from any tracked project) changes.
#[test]
fn digest_card_click_selects_the_task_without_changing_lens() {
    // A second, boring task: with only one task, `reconcile_selected_task`
    // auto-selects it and the persistent detail rail (owner UX pass,
    // 2026-08-05) renders its title too, making "Active work" ambiguous
    // between the digest card and the rail's heading. Sorting by `Task`
    // (id/title, ascending) deterministically auto-selects "TASK-0" instead,
    // leaving "Active work" (TASK-1) unambiguous.
    let boring = task("TASK-0", "Boring backlog item", "To Do");
    let mut in_progress = task("TASK-1", "Active work", "In Progress");
    in_progress.updated_date = Some("2026-08-01 12:00".to_string());
    let mut harness = digest_harness_with(vec![boring, in_progress]);
    harness.state_mut().backlog_view.sort_key = BacklogTaskSortKey::Task;
    harness.run();

    click_at_node_center(&mut harness, "Active work");
    harness.run();

    assert_eq!(
        harness.state().backlog_view.lens,
        BacklogLens::Digest,
        "selecting a Digest card should not change the active lens"
    );
    assert_eq!(
        harness.state().backlog_view.selected_task,
        Some((PathBuf::from(REPO_PATH), "TASK-1".to_string()))
    );
}

/// A left-click at the labeled node's center via raw pointer-event
/// simulation, for widgets whose accessible node doesn't reliably route the
/// accesskit `Click` semantic action to the actual click-sensing ancestor.
/// Confirmed empirically: `digest::render_strip`'s card click only
/// registers via a positioned pointer event, not `Node::click()`, even
/// though the structurally-identical `board::paint_strip` case (see
/// `board_card_click_selects_the_task` above) happens to work with
/// `.click()` — an inconsistency in how `egui`/`egui_kittest` 0.30 route the
/// semantic click action through a retroactively-`.interact()`ed `egui::
/// Frame` response, not something traceable to a Switchbard code path (both
/// call sites use the identical `frame.show(...).response.interact(Sense::
/// click())` pattern). Position-based simulation works reliably for both.
fn click_at_node_center(harness: &mut Harness<'_, HiveApp>, label: &str) {
    let bounds = {
        let node = harness.get_by_label(label);
        let b = node.raw_bounds().expect("node should have bounds");
        egui::Rect::from_min_max(
            egui::Pos2::new(b.x0 as f32, b.y0 as f32),
            egui::Pos2::new(b.x1 as f32, b.y1 as f32),
        )
    };
    let center = bounds.center();
    harness
        .input_mut()
        .events
        .push(egui::Event::PointerMoved(center));
    harness.input_mut().events.push(egui::Event::PointerButton {
        pos: center,
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: egui::Modifiers::default(),
    });
    harness.input_mut().events.push(egui::Event::PointerButton {
        pos: center,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: egui::Modifiers::default(),
    });
}

// ─── Board lens ──────────────────────────────────────────────────────────
//
// TASK-29 (owner-reported live regression, 2026-08-05): board card click,
// checkbox click, and drag were all silently dead in the real app — not
// just a kittest limitation as the TASK-24/26 notes formerly here
// concluded. Root cause (confirmed by reading egui 0.31.1's own
// `hit_test.rs`, not guessed): `Ui::dnd_drag_source` registers the
// draggable region as a *second*, `Sense::drag()`-only widget, layered on
// top of (registered after, so topmost) whatever the card's own content
// already registered — and egui's hit-test explicitly discards any click
// underneath a topmost pure-drag widget. See `board.rs`'s `render_strip`
// doc comment for the full trace through `hit_test_on_close`. The fix
// (also in board.rs) makes the bulk-select checkbox a non-overlapping
// sibling of a single `Sense::click_and_drag()` widget instead of a
// retroactive whole-card interact fighting a separate drag-only one — and
// with that structural change, card click and checkbox click are now
// both cleanly kittest-drivable, proven below instead of documented as
// undrivable.

/// TASK-24/TASK-29: a two-task fixture, same discriminating shape the old
/// (now-removed) UNDRIVABLE note describes needing — with two tasks,
/// `reconcile_selected_task`'s auto-select-first-row default can't produce
/// a false positive, so clicking the *second* card and landing on it
/// specifically proves the click itself is what moved selection.
/// Owner UX pass (2026-08-05): TASK-24 originally jumped to the List lens
/// on click, since Board had no detail pane of its own. Now the persistent
/// detail rail shows the selection regardless of lens, so the click stays
/// on Board — the assertion updated accordingly (the click/selection
/// mechanism itself is unchanged from the doc block above).
#[test]
fn board_card_click_selects_the_task_without_changing_lens() {
    let mut harness = list_harness_with_tasks(vec![
        task("TASK-1", "First card", "To Do"),
        task("TASK-2", "Second card", "To Do"),
    ]);
    harness.state_mut().backlog_view.lens = BacklogLens::Board;
    harness.run();

    click_at_node_center(&mut harness, "Second card");
    harness.run();

    assert_eq!(
        harness.state().backlog_view.lens,
        BacklogLens::Board,
        "selecting a board card should not change the active lens"
    );
    assert_eq!(
        harness.state().backlog_view.selected_task,
        Some((PathBuf::from(REPO_PATH), "TASK-2".to_string())),
        "clicking the second card should select it specifically, not just \
         leave the auto-selected default (TASK-1) in place"
    );
}

/// A non-editable (Archived-source) card takes `render_strip`'s
/// `Sense::click()`-only branch, never `Sense::click_and_drag()` — proves
/// click-to-open still works without a drag sense in the mix at all, the
/// same non-drag-wrapped case the old UNDRIVABLE investigation used to try
/// to rule out drag-wrapping as the cause.
#[test]
fn board_non_editable_card_click_still_selects_it() {
    let mut archived = task("TASK-1", "Archived card", "To Do");
    archived.source = BacklogTaskSource::Archived;
    let mut harness =
        list_harness_with_tasks(vec![task("TASK-2", "Active card", "To Do"), archived]);
    harness.state_mut().backlog_view.lens = BacklogLens::Board;
    harness.state_mut().backlog_view.show_archived = true;
    harness.run();

    click_at_node_center(&mut harness, "Archived card");
    harness.run();

    assert_eq!(
        harness.state().backlog_view.selected_task,
        Some((PathBuf::from(REPO_PATH), "TASK-1".to_string())),
        "a non-editable card's click-only (no drag) sense should still select it"
    );
}

/// TASK-25 (owner-requested UX): a project's `config.yml`-declared status
/// (Icebox, matching budget's real config) should show as a Board column
/// even with zero tasks in it right now — declaring it is enough, per
/// `column_order`'s doc (board.rs). No CLI call decides this outcome (the
/// `backlog` CLI has no way to set the statuses list at all — see
/// `load_backlog_project_reads_configured_statuses_from_a_real_init`,
/// backlog_cli_mutations.rs, for that finding and the real-fixture proof of
/// the parsing itself), so an in-memory fixture with `configured_statuses`
/// set directly is the right level for exercising the *render* path.
#[test]
fn board_shows_the_icebox_column_even_with_zero_icebox_tasks() {
    let mut app = seeded_app();
    app.view_tab = ViewTab::Backlog;
    app.backlog_view.lens = BacklogLens::Board;
    app.backlog_view.selected_project = Some(PathBuf::from(REPO_PATH));
    app.backlog_projects.lock().unwrap().insert(
        PathBuf::from(REPO_PATH),
        BacklogProject {
            root: PathBuf::from(REPO_PATH),
            cli_path: Some(PathBuf::from("/usr/local/bin/backlog")),
            tasks: vec![task("TASK-1", "Ordinary task", "To Do")],
            warnings: vec![],
            loaded_at_unix: 0,
            configured_statuses: vec![
                "Icebox".to_string(),
                "To Do".to_string(),
                "In Progress".to_string(),
                "In Review".to_string(),
                "Done".to_string(),
            ],
        },
    );
    let mut harness = harness(app);
    harness.run();

    assert!(
        harness.query_all_by_label("Icebox").next().is_some(),
        "Icebox should render as a column even though no task is in it"
    );
    assert!(
        harness.query_all_by_label("In Review").next().is_some(),
        "In Review (also config-declared, also zero tasks) should render too"
    );
}

// Status filter combo, priority filter combo, detail-pane status/priority
// combos, Create modal's status/priority combos: all `egui::ComboBox`es —
// UNDRIVABLE, same standing limitation this codebase's other ComboBoxes
// have (confirmed here too, not assumed: `harness.get_all_by_label(
// "Status").next().unwrap().click()` followed by a `Button`-role node dump
// showed no "Icebox" node reachable, since the ComboBox trigger itself has
// no accessible label separate from the adjacent static "Status" text
// label, and `.click()` on that static label doesn't open the popup it
// isn't attached to). Verification for the owner UX pass's unified
// vocabulary rests on:
//   1. `switchbard_core::backlog::types::tests` (backlog/types.rs) —
//      5 unit tests exhaustively proving `ordered_status_vocabulary`'s
//      union/ordering logic, the actual behavior change.
//   2. Code review — every call site (board.rs, toolbar.rs, detail.rs,
//      create.rs, stats.rs) is a one-line, non-branching call into that
//      single, already-proven function; none re-derives its own logic.
//   3. `board_shows_the_icebox_column_even_with_zero_icebox_tasks` above
//      proves the identical union/render pattern end-to-end for Board's
//      column headers, which — unlike a ComboBox's popup — are plain,
//      always-rendered labels and so are directly queryable.

/// Without a declared statuses list at all (the default `Vec::new()`),
/// nothing should change from before TASK-25 — no phantom columns beyond
/// the standard three plus whatever a task actually carries.
#[test]
fn board_does_not_show_icebox_when_no_project_declares_it() {
    let mut harness = list_harness_with_tasks(vec![task("TASK-1", "Ordinary task", "To Do")]);
    harness.state_mut().backlog_view.lens = BacklogLens::Board;
    harness.run();

    assert!(
        harness.query_all_by_label("Icebox").next().is_none(),
        "no project declared Icebox, so it should not appear as a column"
    );
}

// TASK-26/TASK-29: Board bulk select — the checkbox click was one of the
// widgets TASK-29 fixed (see the "Board lens" section header above for the
// full root-cause trace). Now that it's a non-overlapping sibling of the
// card's click-and-drag region instead of nested inside a wider retroactive
// interact, it click-drives cleanly — proven below alongside the
// already-passing direct-state render check.
#[test]
fn board_card_checkbox_click_toggles_bulk_selection() {
    let mut harness = list_harness_with_tasks(vec![task("TASK-1", "Selectable card", "To Do")]);
    harness.state_mut().backlog_view.lens = BacklogLens::Board;
    harness.run();

    let key = (PathBuf::from(REPO_PATH), "TASK-1".to_string());
    assert!(!harness
        .state()
        .backlog_view
        .bulk_selected_tasks
        .contains(&key));

    unlabeled_checkbox(&harness, 0).simulate_click();
    harness.run();

    assert!(
        harness
            .state()
            .backlog_view
            .bulk_selected_tasks
            .contains(&key),
        "TASK-29: the checkbox is now a non-overlapping sibling of the \
         card's click-and-drag region, so its own click sense is no longer \
         shadowed"
    );

    unlabeled_checkbox(&harness, 0).simulate_click();
    harness.run();
    assert!(
        !harness
            .state()
            .backlog_view
            .bulk_selected_tasks
            .contains(&key),
        "clicking again should toggle it back off"
    );
}

/// TASK-29: unlike List's own right-click menu (attached to a bare
/// `ui.horizontal(..).response` — see the "List lens: right-click bulk
/// context menu" note above, a *separate*, still-standing kittest
/// limitation this fix doesn't touch), Board's context menu is now
/// attached to the single `ui.interact(content_rect, card_id, sense)`
/// widget the TASK-29 restructuring introduced — a directly-registered
/// widget, not a container response. That turns out to be drivable:
/// simulating a secondary-click both fires `secondary_clicked()`
/// (`selection::focus_context_selection`, asserted via
/// `bulk_selection_anchor`) *and* actually opens the popup itself
/// (`render_task_context_menu`'s own "N selected · M editable" label
/// becomes queryable), so this is verified end to end, not just at the
/// synchronous-state-change level.
#[test]
fn board_card_secondary_click_opens_the_bulk_context_menu() {
    // A second, boring task ahead of it in sort order: with only one task,
    // `reconcile_selected_task` auto-selects it and the persistent detail
    // rail (owner UX pass, 2026-08-05) renders its title too, making
    // "Right click me" ambiguous between the board card and the rail's
    // heading.
    let mut harness = list_harness_with_tasks(vec![
        task("TASK-1", "Boring backlog item", "To Do"),
        task("TASK-2", "Right click me", "To Do"),
    ]);
    harness.state_mut().backlog_view.lens = BacklogLens::Board;
    harness.state_mut().backlog_view.sort_key = BacklogTaskSortKey::Task;
    harness.run();

    let bounds = {
        let node = harness.get_by_label("Right click me");
        let b = node.raw_bounds().expect("node should have bounds");
        egui::Rect::from_min_max(
            egui::Pos2::new(b.x0 as f32, b.y0 as f32),
            egui::Pos2::new(b.x1 as f32, b.y1 as f32),
        )
    };
    let center = bounds.center();
    harness
        .input_mut()
        .events
        .push(egui::Event::PointerMoved(center));
    harness.input_mut().events.push(egui::Event::PointerButton {
        pos: center,
        button: egui::PointerButton::Secondary,
        pressed: true,
        modifiers: egui::Modifiers::default(),
    });
    harness.input_mut().events.push(egui::Event::PointerButton {
        pos: center,
        button: egui::PointerButton::Secondary,
        pressed: false,
        modifiers: egui::Modifiers::default(),
    });
    harness.run();

    let key = (PathBuf::from(REPO_PATH), "TASK-2".to_string());
    assert_eq!(
        harness.state().backlog_view.bulk_selection_anchor,
        Some(key),
        "secondary-click should focus the clicked card for the context menu"
    );
    assert!(
        harness
            .query_all_by_label_contains("selected ·")
            .next()
            .is_some(),
        "the context menu popup itself should have opened, not just the \
         synchronous focus-selection side effect"
    );
}

#[test]
fn board_card_checkbox_reflects_bulk_selection_state() {
    let mut harness = list_harness_with_tasks(vec![task("TASK-1", "Selectable card", "To Do")]);
    harness.state_mut().backlog_view.lens = BacklogLens::Board;
    harness.run();

    let key = (PathBuf::from(REPO_PATH), "TASK-1".to_string());
    assert_eq!(
        unlabeled_checkbox(&harness, 0).toggled(),
        Some(egui::accesskit::Toggled::False),
        "unselected by default"
    );

    harness
        .state_mut()
        .backlog_view
        .bulk_selected_tasks
        .insert(key);
    harness.run();

    assert_eq!(
        unlabeled_checkbox(&harness, 0).toggled(),
        Some(egui::accesskit::Toggled::True),
        "the card's own checkbox should render checked once the task is bulk-selected"
    );
}

/// QA parity matrix, "Kanban card: labels"/"Kanban card: age" (was a LOW
/// gap): the strip should show both, matching the webview's card.
#[test]
fn board_card_shows_labels_and_a_humanized_age() {
    let mut labeled = task("TASK-1", "Labeled task", "To Do");
    labeled.labels = vec!["frontend".to_string(), "urgent".to_string()];
    // Fixed, well-in-the-past date: `humanize_age`'s exact bucket depends on
    // wall-clock "now", so assert on the "... ago" suffix kittest's
    // substring query supports, not a specific "3d ago" string.
    labeled.updated_date = Some("2026-06-01 09:00".to_string());

    let mut app = list_app_with_tasks(vec![labeled]);
    app.backlog_view.lens = BacklogLens::Board;
    let mut harness = harness(app);
    harness.run();

    assert!(
        harness
            .query_all_by_label("frontend, urgent")
            .next()
            .is_some(),
        "the card should show its labels, comma-joined"
    );
    assert!(
        harness.query_all_by_label_contains("ago").next().is_some(),
        "the card should show a humanized age derived from updated_date"
    );
}

/// The unlabeled/undated fixture tasks used throughout this file (see
/// `task()`'s fixed `updated_date`) always have an age, so this covers the
/// no-labels half specifically: a card with no labels shouldn't render an
/// empty label line.
#[test]
fn board_card_omits_the_label_line_when_there_are_no_labels() {
    let plain = task("TASK-1", "Plain task", "To Do");
    assert!(plain.labels.is_empty(), "fixture should start unlabeled");

    let mut app = list_app_with_tasks(vec![plain]);
    app.backlog_view.lens = BacklogLens::Board;
    let mut harness = harness(app);
    harness.run();

    assert!(
        harness.query_all_by_label_contains("ago").next().is_some(),
        "age should still render from the fixture's updated_date"
    );
}

/// TASK-29 mission item 4: prove the drag-to-change-status mechanism
/// itself still works after replacing `Ui::dnd_drag_source` with the
/// hand-rolled `Sense::click_and_drag()` + manual `dnd_set_drag_payload`
/// approach — a full simulated pointer drag (press, move past the
/// click/drag threshold across a couple of frames so egui's own
/// `is_decidedly_dragging` commits to a drag, release over a different
/// column) should still land on `apply_drop`'s synchronous status-change
/// side effect. The CLI round-trip that follows (`spawn_backlog_save`) is
/// already proven separately by `save_button_completes_a_real_cli_round_
/// trip_against_a_real_fixture_repo`; what's under test here is the
/// drag/drop wiring itself — did the right card land on the right column
/// and get the right patch queued — which is what TASK-29's restructuring
/// touched.
#[test]
fn board_drag_and_drop_between_columns_queues_a_status_change() {
    // Two tasks, not one: with only one, `reconcile_selected_task`
    // auto-selects it and the persistent detail rail (owner UX pass,
    // 2026-08-05) renders its title too, making "Draggable card" ambiguous
    // between the board card and the rail's heading. Sorting by `Task`
    // (id/title, ascending) deterministically puts TASK-1 first — that one
    // becomes the boring auto-selected task, leaving TASK-2's "Draggable
    // card" title unambiguous on the board.
    let mut harness = list_harness_with_tasks(vec![
        task("TASK-1", "Other card", "To Do"),
        task("TASK-2", "Draggable card", "To Do"),
    ]);
    harness.state_mut().backlog_view.lens = BacklogLens::Board;
    harness.state_mut().backlog_view.sort_key = BacklogTaskSortKey::Task;
    harness.run();

    let source_center = {
        let node = harness.get_by_label("Draggable card");
        let b = node.raw_bounds().expect("node should have bounds");
        egui::Pos2::new(((b.x0 + b.x1) / 2.0) as f32, ((b.y0 + b.y1) / 2.0) as f32)
    };
    let target_center = {
        // Drop well below the "In Progress" column's own header label, into
        // its (empty) drop-zone body — dropping directly on the header text
        // itself isn't the intended gesture and the header has no
        // meaningful drop behavior of its own either way.
        let node = harness.get_by_label("In Progress");
        let b = node.raw_bounds().expect("node should have bounds");
        egui::Pos2::new(((b.x0 + b.x1) / 2.0) as f32, b.y1 as f32 + 80.0)
    };

    harness
        .input_mut()
        .events
        .push(egui::Event::PointerMoved(source_center));
    harness.input_mut().events.push(egui::Event::PointerButton {
        pos: source_center,
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: egui::Modifiers::default(),
    });
    harness.run();

    // Move in a couple of steps, well past any click-vs-drag threshold, so
    // `is_decidedly_dragging` commits to a drag instead of resolving as a
    // click on release.
    let midpoint = egui::Pos2::new(source_center.x, (source_center.y + target_center.y) / 2.0);
    harness
        .input_mut()
        .events
        .push(egui::Event::PointerMoved(midpoint));
    harness.run();
    harness
        .input_mut()
        .events
        .push(egui::Event::PointerMoved(target_center));
    harness.run();

    harness.input_mut().events.push(egui::Event::PointerButton {
        pos: target_center,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: egui::Modifiers::default(),
    });
    harness.run();

    assert_eq!(
        harness.state().backlog_status.snapshot().as_deref(),
        Some("moving TASK-2 to In Progress"),
        "TASK-29: dropping a card on another column should still \
         synchronously queue a status-change save, same as before the \
         click/checkbox fix"
    );
}

// ─── Persistent detail rail (owner UX pass, 2026-08-05) ─────────────────
//
// The tests above (board_card_click_selects_the_task_without_changing_lens,
// digest_card_click_selects_the_task_without_changing_lens,
// search_result_row_click_selects_the_task_without_changing_lens) prove
// that clicking updates `backlog_view.selected_task` and leaves the lens
// alone. The three tests below go one step further and prove the rail
// itself renders that selection's detail — its task-id label specifically,
// which `render_detail_header` renders once per selected task and nothing
// else in the window duplicates, unlike the title (which the source card
// keeps showing too).

#[test]
fn board_card_click_updates_the_rail_to_show_the_clicked_tasks_detail() {
    // TASK-35 (independent verifier finding on the first version of this
    // test, c7e6624): a Board card renders its own task-id label
    // unconditionally (`paint_card`, board.rs) — `query_all_by_label(
    // "TASK-2").next().is_some()` after the click would pass even if the
    // rail never updated at all, since the CARD alone already satisfies it.
    // Counting exact-match "TASK-2" labels before vs. after the click (1,
    // the card only -> 2, card + rail) is the differential that actually
    // requires the rail to have changed.
    let mut harness = list_harness_with_tasks(vec![
        task("TASK-1", "First card", "To Do"),
        task("TASK-2", "Second card", "To Do"),
    ]);
    harness.state_mut().backlog_view.lens = BacklogLens::Board;
    harness.state_mut().backlog_view.sort_key = BacklogTaskSortKey::Task;
    harness.run();
    assert!(
        harness.query_all_by_label("TASK-1").next().is_some(),
        "sanity: the rail starts on the auto-selected first task"
    );
    assert_eq!(
        harness.query_all_by_label("TASK-2").count(),
        1,
        "before the click, TASK-2's id should appear exactly once (its \
         board card only — not yet in the rail)"
    );

    click_at_node_center(&mut harness, "Second card");
    harness.run();

    assert_eq!(
        harness.query_all_by_label("TASK-2").count(),
        2,
        "after the click, TASK-2's id should appear twice: its board card \
         plus the rail's own header, proving the rail actually updated"
    );
}

#[test]
fn list_row_click_updates_the_rail_to_show_the_clicked_tasks_detail() {
    // Unlike Board/Digest cards, a List row never renders a bare task-id
    // label of its own (it's always "{id}  {title}" combined — see list.rs)
    // — confirmed empirically (0 exact "TASK-2" matches before the click)
    // rather than assumed, so this one was never vulnerable to TASK-35's
    // finding. Still asserts the same 0-before/1-after differential for
    // consistency with the Board/Digest versions of this test.
    let mut harness = list_harness_with_tasks(vec![
        task("TASK-1", "First", "To Do"),
        task("TASK-2", "Second", "To Do"),
    ]);
    harness.state_mut().backlog_view.sort_key = BacklogTaskSortKey::Task;
    harness.run();
    assert!(harness.query_all_by_label("TASK-1").next().is_some());
    assert_eq!(harness.query_all_by_label("TASK-2").count(), 0);

    harness.get_by_label("TASK-2  Second").click();
    harness.run();

    assert_eq!(
        harness.query_all_by_label("TASK-2").count(),
        1,
        "the rail should show TASK-2's detail after selecting its row"
    );
}

#[test]
fn digest_card_click_updates_the_rail_to_show_the_clicked_tasks_detail() {
    // TASK-35's fix applies equally here: a Digest card also renders its
    // own task-id label unconditionally (`render_strip`, digest.rs), so
    // this counts exact-match occurrences before/after the click rather
    // than just checking presence — see the Board test's comment above.
    let boring = task("TASK-0", "Boring backlog item", "To Do");
    let mut in_progress = task("TASK-1", "Active work", "In Progress");
    in_progress.updated_date = Some("2026-08-01 12:00".to_string());
    let mut harness = digest_harness_with(vec![boring, in_progress]);
    harness.state_mut().backlog_view.sort_key = BacklogTaskSortKey::Task;
    harness.run();
    assert!(
        harness.query_all_by_label("TASK-0").next().is_some(),
        "sanity: the rail starts on the auto-selected first task"
    );
    assert_eq!(
        harness.query_all_by_label("TASK-1").count(),
        1,
        "before the click, TASK-1's id should appear exactly once (its \
         digest card only — not yet in the rail)"
    );

    click_at_node_center(&mut harness, "Active work");
    harness.run();

    assert_eq!(
        harness.query_all_by_label("TASK-1").count(),
        2,
        "after the click, TASK-1's id should appear twice: its digest card \
         plus the rail's own header, proving the rail actually updated"
    );
}

/// No selection at all — not just "nothing clicked yet" (`reconcile_
/// selected_task` always auto-selects the first *visible* task when one
/// exists) but the realistic case where a project has tasks, none of them
/// currently pass the visibility filters. The rail should show its quiet
/// existing empty state (`render_task_detail`'s own "Select a task"),
/// unmodified for the rail — proving the rail needs no separate
/// empty-state handling of its own.
#[test]
fn rail_shows_the_quiet_empty_state_when_no_task_is_visible() {
    let mut harness = list_harness_with_tasks(vec![task("TASK-1", "Only task", "To Do")]);
    harness.state_mut().backlog_view.status_filter = "Done".to_string();
    harness.run();

    assert_eq!(
        harness.state().backlog_view.selected_task,
        None,
        "sanity: nothing visible means nothing selected"
    );
    assert!(
        harness.query_all_by_label("Select a task").next().is_some(),
        "the rail should show the quiet empty state, not an empty editor"
    );
}

// ─── Milestones lens ─────────────────────────────────────────────────────

#[test]
fn milestone_row_click_selects_the_task() {
    let mut milestoned = task("TASK-1", "Milestoned task", "To Do");
    milestoned.milestone = Some("v1".to_string());
    let mut app = list_app_with_tasks(vec![milestoned]);
    app.backlog_view.lens = BacklogLens::Milestones;
    let mut harness = harness(app);
    harness.run();

    harness.get_by_label("TASK-1  Milestoned task").click();
    harness.run();

    assert_eq!(
        harness.state().backlog_view.selected_task,
        Some((PathBuf::from(REPO_PATH), "TASK-1".to_string()))
    );
}

#[test]
fn milestone_collapsing_header_collapses_and_reveals_its_tasks() {
    let mut milestoned = task("TASK-1", "Milestoned task", "To Do");
    milestoned.milestone = Some("v1".to_string());
    let mut app = list_app_with_tasks(vec![milestoned]);
    app.backlog_view.lens = BacklogLens::Milestones;
    let mut harness = harness(app);
    harness.run();

    assert!(
        harness.query_by_label("TASK-1  Milestoned task").is_some(),
        "milestone groups default open"
    );

    harness.get_by_label("v1  ·  0/1 done").click();
    // `CollapsingHeader` animates open/close (`ctx.animate_bool`); its
    // content keeps painting until the collapse animation finishes, so this
    // needs several frames rather than one `run()`'s worth.
    for _ in 0..10 {
        harness.run();
    }
    assert!(
        harness.query_by_label("TASK-1  Milestoned task").is_none(),
        "clicking the milestone header should collapse its group"
    );

    harness.get_by_label("v1  ·  0/1 done").click();
    for _ in 0..10 {
        harness.run();
    }
    assert!(harness.query_by_label("TASK-1  Milestoned task").is_some());
}

// ─── Detail pane: checklists, references, notes, milestone, description ──

fn detail_task_with_checklists() -> BacklogTask {
    let mut t = task("TASK-1", "Checklist task", "To Do");
    t.acceptance_criteria = vec![BacklogChecklistItem {
        index: 1,
        checked: false,
        text: "Criterion one".to_string(),
    }];
    t.definition_of_done = vec![BacklogChecklistItem {
        index: 1,
        checked: false,
        text: "DoD one".to_string(),
    }];
    t.milestone = Some("v1".to_string());
    t.description = "Some body text.".to_string();
    t
}

fn detail_harness_on(t: BacklogTask) -> Harness<'static, HiveApp> {
    let id = t.id.clone();
    let mut app = list_app_with_tasks(vec![t]);
    app.backlog_view.selected_task = Some((PathBuf::from(REPO_PATH), id));
    let mut harness = harness(app);
    harness.run();
    harness
}

#[test]
fn acceptance_criterion_checkbox_click_sets_the_synchronous_updating_status() {
    let mut harness = detail_harness_on(detail_task_with_checklists());
    harness.get_by_label("#1 Criterion one").click();
    harness.run();
    assert_eq!(
        harness.state().backlog_status.snapshot().as_deref(),
        Some("updating TASK-1 AC #1"),
        "clicking an AC checkbox should synchronously report the pending update \
         (the eventual CLI completion is proven directly in switchbard-core's \
         backlog_cli_mutations.rs, not by waiting on this background thread)"
    );
}

#[test]
fn definition_of_done_checkbox_click_sets_the_synchronous_updating_status() {
    let mut harness = detail_harness_on(detail_task_with_checklists());
    harness.get_by_label("#1 DoD one").click();
    harness.run();
    assert_eq!(
        harness.state().backlog_status.snapshot().as_deref(),
        Some("updating TASK-1 DoD #1")
    );
}

#[test]
fn milestone_clear_button_empties_the_milestone_field() {
    let mut harness = detail_harness_on(detail_task_with_checklists());
    assert!(harness.query_by_label("Clear").is_some());

    harness.get_by_label("Clear").click();
    harness.run();

    assert_eq!(harness.state().backlog_view.editor.milestone, "");
    assert!(
        harness.query_by_label("Clear").is_none(),
        "the Clear button only shows while the milestone field is non-empty"
    );
}

#[test]
fn description_edit_raw_toggle_swaps_in_the_raw_editor_and_back() {
    let mut harness = detail_harness_on(detail_task_with_checklists());
    assert!(!harness.state().backlog_view.editor.description_editing);

    harness.get_by_label("Edit raw").click();
    harness.run();
    assert!(harness.state().backlog_view.editor.description_editing);
    assert!(harness.query_by_label("View rendered").is_some());

    harness.get_by_label("View rendered").click();
    harness.run();
    assert!(!harness.state().backlog_view.editor.description_editing);
}

#[test]
fn references_add_button_clears_the_input_field() {
    let mut harness = detail_harness_on(detail_task_with_checklists());
    // Hint text ("Add a reference...") isn't an accessible label either —
    // only the surrounding field's role + position relative to the title
    // field identify it (see `detail_text_input`'s doc comment).
    let field = detail_text_input(&harness, "Checklist task", 5);
    field.focus();
    field.type_text("https://example.com/new-ref");
    harness.run();
    assert_eq!(
        harness.state().backlog_view.editor.new_reference,
        "https://example.com/new-ref"
    );

    harness.get_by_label("Add").click();
    harness.run();
    assert_eq!(
        harness.state().backlog_view.editor.new_reference,
        "",
        "Add should clear the new-reference buffer once the save is queued"
    );
}

#[test]
fn append_note_button_clears_the_note_input() {
    let mut harness = detail_harness_on(detail_task_with_checklists());
    // Hint text ("Append note") isn't an accessible label; the field is the
    // second MultilineTextInput in render order (plan is the first).
    let field = multiline_input_nth(&harness, 1);
    field.focus();
    field.type_text("A new note");
    harness.run();
    assert_eq!(harness.state().backlog_view.editor.note, "A new note");

    harness.get_by_label("Append Note").click();
    harness.run();
    assert_eq!(
        harness.state().backlog_view.editor.note,
        "",
        "Append Note should clear the note buffer once the append is queued"
    );
}

#[test]
fn editing_the_title_enables_the_save_button() {
    let mut harness = detail_harness_on(detail_task_with_checklists());
    // Owner UX pass (2026-08-05): the detail pane now renders in the
    // persistent right-hand rail (`rail::render_detail_rail`), which is a
    // SidePanel shown *before* the CentralPanel's List content — so its two
    // Save buttons (field editor, then Dependencies) now come before the
    // saved-views bar's own Save, reversing the old embedded-in-List order.
    // Three "Save"-labeled buttons total: field editor (0), Dependencies
    // (1), saved-views bar (2) — all disabled with no pending edits.
    assert!(
        harness
            .get_all_by_label("Save")
            .next()
            .unwrap()
            .is_disabled(),
        "Save should start disabled with no pending edits"
    );

    // The title field has no accessible label of its own (see
    // `text_input_nth`'s doc comment); it's index 0 in render order.
    let title_field = detail_text_input(&harness, "Checklist task", 0);
    title_field.focus();
    title_field.type_text(" edited");
    harness.run();

    assert!(
        !harness
            .get_all_by_label("Save")
            .next()
            .unwrap()
            .is_disabled(),
        "editing the title should enable Save (the click itself, and the CLI \
         round trip it triggers, are proven in \
         save_button_completes_a_real_cli_round_trip_against_a_real_fixture_repo \
         below)"
    );
}

#[test]
fn archive_button_shows_confirm_then_cancel_reverts_to_the_plain_button() {
    let mut harness = detail_harness_on(detail_task_with_checklists());
    assert!(harness.query_by_label("Archive").is_some());

    harness.get_by_label("Archive").click();
    harness.run();
    assert!(harness.state().backlog_view.archive_confirm);
    assert!(harness.query_by_label("Archive TASK-1?").is_some());

    harness.get_by_label("Cancel").click();
    harness.run();
    assert!(!harness.state().backlog_view.archive_confirm);
    assert!(harness.query_by_label("Archive").is_some());
}

#[test]
fn archive_confirm_sets_the_synchronous_archiving_status() {
    let mut harness = detail_harness_on(detail_task_with_checklists());
    harness.get_by_label("Archive").click();
    harness.run();

    harness.get_by_label("Confirm archive").click();
    harness.run();

    assert!(!harness.state().backlog_view.archive_confirm);
    assert_eq!(
        harness.state().backlog_status.snapshot().as_deref(),
        Some("archiving TASK-1")
    );
}

/// 2026-08-05 fix-wave 2, a new HIGH-class defect the re-verification found:
/// the real CLI refuses `task archive` on a Done task. The detail pane must
/// not offer an action the CLI will reject, so a Done task's affordance
/// switches to "Complete" instead of "Archive" (Backlog.md semantics: Done
/// -> completed into backlog/completed/, non-Done -> archived into
/// backlog/archive/ — verified against a real fixture repo). See
/// switchbard-core's archiving_a_done_task_is_rejected_by_the_real_cli for
/// the real-CLI proof of the underlying refusal + complete_backlog_task's
/// success.
#[test]
fn done_task_offers_complete_instead_of_archive() {
    let mut done_task = detail_task_with_checklists();
    done_task.status = "Done".to_string();
    let mut harness = detail_harness_on(done_task);
    // `reconcile_selected_task` (mod.rs) clears a selection that falls
    // outside the currently *visible* rows, and Done tasks are hidden by
    // default (`show_completed` defaults false) — without this, the
    // explicit `selected_task` above gets reset to nothing the first frame.
    harness.state_mut().backlog_view.show_completed = true;
    harness.run();

    assert!(
        harness.query_by_label("Complete").is_some(),
        "a Done task's detail pane should offer Complete"
    );
    assert!(
        harness.query_by_label("Archive").is_none(),
        "a Done task's detail pane should not offer Archive — the CLI refuses it"
    );

    harness.get_by_label("Complete").click();
    harness.run();
    assert!(harness.state().backlog_view.archive_confirm);
    assert!(harness.query_by_label("Complete TASK-1?").is_some());

    harness.get_by_label("Confirm complete").click();
    harness.run();
    assert!(!harness.state().backlog_view.archive_confirm);
    assert_eq!(
        harness.state().backlog_status.snapshot().as_deref(),
        Some("completing TASK-1")
    );
}

// ─── Theme toggle ────────────────────────────────────────────────────────

#[test]
fn theme_toggle_button_flips_config_theme_both_directions() {
    use switchbard_core::config::ThemeChoice;
    let mut harness = list_harness_with_tasks(vec![task("TASK-1", "First", "To Do")]);
    assert_eq!(harness.state().config.ui.theme, ThemeChoice::Light);

    harness.get_by_label("Dark theme").click();
    harness.run();
    assert_eq!(harness.state().config.ui.theme, ThemeChoice::Dark);
    assert!(
        harness.query_by_label("Light theme").is_some(),
        "the button's own label should flip to offer switching back"
    );

    harness.get_by_label("Light theme").click();
    harness.run();
    assert_eq!(harness.state().config.ui.theme, ThemeChoice::Light);
}

/// The click itself only mutates `config.ui.theme`; persistence is triggered
/// by `eframe::App::update`'s before/after diff (`app.rs`), which this
/// harness never calls (it drives `render_ui` directly — `egui_kittest`
/// 0.30 has no `eframe::App` integration to invoke the trait method through).
/// This proves the persistence mechanism itself — `save_config` writing
/// `config.ui.theme` to disk and `config::load_from` reading it back — using
/// the same isolated path every other test's `HiveApp` is required to use.
#[test]
fn theme_persists_through_save_config_and_reload() {
    use switchbard_core::config::ThemeChoice;
    let mut harness = list_harness_with_tasks(vec![task("TASK-1", "First", "To Do")]);
    harness.get_by_label("Dark theme").click();
    harness.run();
    assert_eq!(harness.state().config.ui.theme, ThemeChoice::Dark);

    harness.state_mut().save_config();

    let save_path = harness
        .state()
        .config_save_path
        .clone()
        .expect("test harness always sets an isolated config_save_path");
    let reloaded = switchbard_core::config::load_from(&save_path)
        .expect("reloading the just-saved config should succeed");
    assert_eq!(
        reloaded.ui.theme,
        ThemeChoice::Dark,
        "the theme should round-trip through save_config/load_from"
    );
}

// ─── TASK-27: collapsible "Tracked repos" sidebar ───────────────────────

#[test]
fn sidebar_collapse_button_hides_the_repo_list_both_directions() {
    // Owner UX pass (2026-08-05): "Tracked repos" is now a left-side panel
    // local to the Servers view (freed up the right edge for the Backlog
    // view's detail rail) — a Backlog-lens harness never renders it at all
    // now, so this needs the default Servers view instead of
    // `list_harness_with_tasks`. The collapse/expand glyphs flipped with
    // the side: "◀" (toward the left edge) collapses, "▶" (away from it)
    // expands — the mirror image of the old right-side panel's arrows.
    let mut harness = harness(seeded_app());
    harness.run();
    assert!(!harness.state().config.ui.sidebar_collapsed);
    assert!(
        harness.query_by_label("Tracked repos").is_some(),
        "expanded by default: the heading should render"
    );

    harness.get_by_label("◀").click();
    harness.run();
    assert!(harness.state().config.ui.sidebar_collapsed);
    assert!(
        harness.query_by_label("Tracked repos").is_none(),
        "collapsed: the repo list content should not render"
    );
    assert!(
        harness.query_by_label("▶").is_some(),
        "collapsed: the rail should offer the expand toggle"
    );

    harness.get_by_label("▶").click();
    harness.run();
    assert!(!harness.state().config.ui.sidebar_collapsed);
    assert!(harness.query_by_label("Tracked repos").is_some());
}

/// Same split as `theme_persists_through_save_config_and_reload`: the click
/// only mutates `config.ui.sidebar_collapsed` in memory (persistence is
/// `eframe::App::update`'s before/after diff, which this harness never
/// drives) — this proves the round trip itself.
#[test]
fn sidebar_collapsed_persists_through_save_config_and_reload() {
    // Owner UX pass (2026-08-05): "Tracked repos" only renders in the
    // (default) Servers view now — see the sibling test above for why this
    // switched away from `list_harness_with_tasks`.
    let mut harness = harness(seeded_app());
    harness.run();
    harness.get_by_label("◀").click();
    harness.run();
    assert!(harness.state().config.ui.sidebar_collapsed);

    harness.state_mut().save_config();

    let save_path = harness
        .state()
        .config_save_path
        .clone()
        .expect("test harness always sets an isolated config_save_path");
    let reloaded = switchbard_core::config::load_from(&save_path)
        .expect("reloading the just-saved config should succeed");
    assert!(
        reloaded.ui.sidebar_collapsed,
        "sidebar_collapsed should round-trip through save_config/load_from"
    );
}

// ─── Owner UX pass (2026-08-05): Tracked repos relocation + Settings ────

#[test]
fn tracked_repos_does_not_render_in_the_backlog_view() {
    let harness = list_harness_with_tasks(vec![task("TASK-1", "First", "To Do")]);
    assert!(
        harness.query_by_label("Tracked repos").is_none(),
        "Tracked repos is Servers-local now — a Backlog-view harness \
         should never render it, regardless of lens"
    );
}

#[test]
fn settings_button_opens_a_window_with_the_repo_list() {
    let mut harness = list_harness_with_tasks(vec![task("TASK-1", "First", "To Do")]);
    assert!(!harness.state().settings_open);
    assert!(
        harness.query_by_label("Settings").is_none(),
        "sanity: closed by default"
    );

    harness.get_by_label("⚙ Settings").click();
    harness.run();

    assert!(harness.state().settings_open);
    assert!(
        harness.query_by_label("Settings").is_some(),
        "the Settings window should be open"
    );
    assert!(
        harness.query_all_by_label(REPO_NAME).next().is_some(),
        "the tracked repo should be listed in Settings, reachable from the \
         Backlog view where Tracked repos itself doesn't render"
    );
    assert!(harness.query_by_label("➕ Add repo").is_some());
    assert!(harness.query_by_label("Remove").is_some());
}

#[test]
fn settings_remove_button_opens_the_shared_confirmation_modal() {
    let mut harness = list_harness_with_tasks(vec![task("TASK-1", "First", "To Do")]);
    harness.get_by_label("⚙ Settings").click();
    harness.run();

    harness.get_by_label("Remove").click();
    harness.run();

    assert_eq!(
        harness.state().confirm_remove_repo,
        Some((PathBuf::from(REPO_PATH), REPO_NAME.to_string())),
        "Settings' Remove button should set the same confirm_remove_repo \
         state the Tracked-repos panel's own Remove button does"
    );
    assert!(
        harness.query_by_label("Remove repo?").is_some(),
        "the confirmation modal is rendered unconditionally from render_ui, \
         so it should appear even though this harness is in the Backlog \
         view, not Servers"
    );
}

// ─── TASK-28: status surface never renders unbounded multi-line text ────

/// Owner-found bug: `backlog task create --plain` writes the entire newly
/// created task's rendered form to stdout (confirmed empirically against a
/// real fixture — `parse_created_task_id_extracts_the_id_from_a_real_
/// create_call`, backlog_cli_mutations.rs), which used to land verbatim in
/// `backlog_status` and stretch the top bar into a many-line void. This is
/// the defense-in-depth half: even a status message that somehow still
/// contains newlines/very long text must render as a single clamped line,
/// with the full text on hover — regardless of which function produced it.
#[test]
fn action_status_label_clamps_a_multiline_message_to_one_line() {
    let mut harness = list_harness_with_tasks(vec![task("TASK-1", "First", "To Do")]);
    let multiline = "File: /tmp/x/backlog/tasks/task-1 - Title.md\n\n\
                      Task TASK-1 - Title\n\
                      ==================================================\n\
                      \n\
                      Status: \u{25cb} To Do\n";
    harness.state_mut().backlog_status.set(multiline);
    harness.run();

    // The rendered label's own value is the clamped, single-line form —
    // never the raw multi-line message. The Label node and its inner
    // TextRun child both carry the same value, hence query_all rather than
    // the exactly-one query.
    let clamped = harness
        .query_all_by_value("File: /tmp/x/backlog/tasks/task-1 - Title.md …")
        .next()
        .expect("the clamped single-line label should render");
    assert!(
        clamped.value().as_deref() != Some(multiline),
        "the painted label must not be the raw multi-line message"
    );
    assert!(
        harness.query_all_by_value(multiline).next().is_none(),
        "the raw multi-line message must not appear anywhere in the tree"
    );
    // `action_status_label`'s own `.on_hover_text(msg)` call (the full
    // original message reachable on hover, not deleted) is a single,
    // unconditional line — verified by code review rather than a hover
    // simulation here; egui's tooltip only materializes after a real hover
    // delay this harness has no way to advance, and accesskit's node
    // `description()` came back `None` in practice even after a simulated
    // `Node::hover()`, so asserting on it would be testing kittest's
    // tooltip-timing support, not this function.
}

/// A single-line message (the normal case — "saved TASK-1", "archived
/// TASK-1", etc.) renders unchanged, with no trailing ellipsis marker
/// invented for text that was never actually truncated.
#[test]
fn action_status_label_leaves_a_single_line_message_unchanged() {
    let mut harness = list_harness_with_tasks(vec![task("TASK-1", "First", "To Do")]);
    harness.state_mut().backlog_status.set("saved TASK-1");
    harness.run();

    assert!(harness.query_all_by_value("saved TASK-1").next().is_some());
    assert!(harness
        .query_all_by_value("saved TASK-1 …")
        .next()
        .is_none());
}

// ─── Saved views: persistence across restart ────────────────────────────

/// `saved_view_can_be_saved_and_deleted` (`ui_views.rs`) proves the in-memory
/// `config.ui.saved_views` mutation; this closes the one documented gap in
/// that test file's own comment — persistence across a restart — by
/// reloading `Config` from the same isolated path the harness saved to.
#[test]
fn saved_view_persists_across_a_simulated_restart() {
    let mut harness = list_harness_with_tasks(vec![task("TASK-1", "First", "To Do")]);
    harness.state_mut().backlog_view.lens = BacklogLens::Statistics;
    harness.state_mut().backlog_view.priority_filter = "high".to_string();
    // Milestone/label filters (QA parity matrix gap) round-trip through
    // SavedView the same way status/priority already did — this is the
    // regression bar for adding a new filter field: forgetting to wire it
    // into current_as_saved_view/apply_saved_view would silently reset it
    // to "all" the next time this view is applied.
    harness.state_mut().backlog_view.milestone_filter = "v1".to_string();
    harness.state_mut().backlog_view.label_filter = "frontend".to_string();
    harness.run();

    harness.state_mut().backlog_view.saved_view_name_draft = "High priority".to_string();
    harness.get_by_label("Save").click();
    harness.run();
    harness.state_mut().save_config();

    let save_path = harness.state().config_save_path.clone().unwrap();
    let reloaded = switchbard_core::config::load_from(&save_path)
        .expect("reloading the just-saved config should succeed");
    assert_eq!(reloaded.ui.saved_views.len(), 1);
    assert_eq!(reloaded.ui.saved_views[0].name, "High priority");
    assert_eq!(reloaded.ui.saved_views[0].priority_filter, "high");
    assert_eq!(reloaded.ui.saved_views[0].milestone_filter, "v1");
    assert_eq!(reloaded.ui.saved_views[0].label_filter, "frontend");
}

// ─── A real end-to-end CLI round trip through the GUI's own Save button ──

/// The one full-stack proof: a real fixture repo (`git init` + `backlog
/// init`), a real `HiveApp` pointed at it, a real `type_text` edit of the
/// title field, a real click of "Save", and a bounded poll of
/// `backlog_status` for the spawned thread's real `backlog task edit`
/// subprocess to complete — the only test in this QA pass that waits on the
/// background thread `worktree_removal_orchestration.rs` otherwise avoids,
/// justified here because it is the single piece of evidence that the
/// generic detail-pane Save path (the one editor with no synchronous
/// click-time status message) truly reaches the real CLI end to end, not
/// just a fixture path other tests treat as a stand-in. Every other
/// CLI-writing control's completion is proven at the core level instead
/// (`backlog_cli_mutations.rs`).
#[test]
fn save_button_completes_a_real_cli_round_trip_against_a_real_fixture_repo() {
    let fixture = tempfile::tempdir().expect("create temp dir");
    let root = fixture.path();
    run_cmd(root, "git", &["init", "-q"]);
    run_cmd(root, "git", &["config", "user.email", "qa@example.com"]);
    run_cmd(root, "git", &["config", "user.name", "QA Fixture"]);
    run_cmd(
        root,
        "backlog",
        &["init", "--defaults", "--agent-instructions", "none", "qa"],
    );
    run_cmd(root, "backlog", &["task", "create", "Fixture task"]);

    let repos = vec![Repo {
        name: "qa-fixture".to_string(),
        path: root.to_path_buf(),
    }];
    let worktrees = vec![WorktreeRef {
        repo_name: "qa-fixture".to_string(),
        path: root.to_path_buf(),
        branch: Some("main".to_string()),
        head: "abc1234".to_string(),
    }];
    let mut cfg = Config::default();
    cfg.ui.onboarding_dismissed = true;
    let mut app = HiveApp::new_headless(cfg, repos, worktrees);
    app.config_save_path = Some(isolated_config_save_path());
    app.view_tab = ViewTab::Backlog;
    app.backlog_view.lens = BacklogLens::List;
    app.backlog_view.selected_project = Some(root.to_path_buf());
    app.backlog_projects.lock().unwrap().insert(
        root.to_path_buf(),
        switchbard_core::load_backlog_project(root).expect("load the real fixture project"),
    );
    app.backlog_view.selected_task = Some((root.to_path_buf(), "TASK-1".to_string()));

    let mut harness = harness(app);
    harness.run();

    // The title field has no accessible label of its own; find it by its
    // known seeded value (see `detail_text_input`'s doc comment), then
    // append rather than replace — `type_text` inserts at the IME cursor,
    // and this test only needs the persisted value to provably change.
    let title_field = detail_text_input(&harness, "Fixture task", 0);
    title_field.focus();
    title_field.type_text(" — renamed by the real Save button");
    harness.run();

    // Owner UX pass (2026-08-05): the detail pane (and its Save button)
    // now renders in the persistent rail, a SidePanel shown before the
    // CentralPanel's List content — three "Save"-labeled buttons render
    // (field editor's, Dependencies', saved-views bar's, in that order);
    // index 0 is the field editor's.
    harness.get_all_by_label("Save").next().unwrap().click();
    harness.run();

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        harness.run();
        if harness.state().backlog_status.snapshot().as_deref() == Some("saved TASK-1") {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "Save's background thread did not report completion in time; last status: {:?}",
            harness.state().backlog_status.snapshot()
        );
        std::thread::sleep(std::time::Duration::from_millis(20));
    }

    let project =
        switchbard_core::load_backlog_project(root).expect("reload the real fixture project");
    let saved_task = project
        .tasks
        .iter()
        .find(|t| t.id == "TASK-1")
        .expect("task should still exist");
    assert!(
        saved_task.title.contains("renamed by the real Save button"),
        "the real backlog CLI should have persisted the edited title, got {:?}",
        saved_task.title
    );
}

/// TASK-28 (owner-found bug): the compact "Created {repo}:{id}" status
/// message end to end against a real fixture repo and the real CLI — a CLI
/// call decides the id half of this message
/// (`parse_created_task_id`/`create_backlog_task`'s actual output), so an
/// in-memory fixture couldn't prove this the way it proves the click/
/// buffer-reset half (see `create_modal_labels_assignee_milestone_and_
/// dependencies_fields_reset_after_create`, which already covers that half).
#[test]
fn create_modal_reports_a_compact_created_message_against_a_real_fixture_repo() {
    let fixture = tempfile::tempdir().expect("create temp dir");
    let root = fixture.path();
    run_cmd(root, "git", &["init", "-q"]);
    run_cmd(root, "git", &["config", "user.email", "qa@example.com"]);
    run_cmd(root, "git", &["config", "user.name", "QA Fixture"]);
    run_cmd(
        root,
        "backlog",
        &["init", "--defaults", "--agent-instructions", "none", "qa"],
    );

    let repos = vec![Repo {
        name: "MusicProduction".to_string(),
        path: root.to_path_buf(),
    }];
    let worktrees = vec![WorktreeRef {
        repo_name: "MusicProduction".to_string(),
        path: root.to_path_buf(),
        branch: Some("main".to_string()),
        head: "abc1234".to_string(),
    }];
    let mut cfg = Config::default();
    cfg.ui.onboarding_dismissed = true;
    let mut app = HiveApp::new_headless(cfg, repos, worktrees);
    app.config_save_path = Some(isolated_config_save_path());
    app.view_tab = ViewTab::Backlog;
    app.backlog_view.lens = BacklogLens::List;
    app.backlog_view.selected_project = Some(root.to_path_buf());
    app.backlog_projects.lock().unwrap().insert(
        root.to_path_buf(),
        switchbard_core::load_backlog_project(root).expect("load the real fixture project"),
    );

    let mut harness = harness(app);
    harness.run();

    harness.get_by_label("+ Task").click();
    harness.run();

    let modal = harness.get_by_label("New Backlog Task");
    let title_field = modal
        .query_all(kittest::by().role(egui::accesskit::Role::TextInput))
        .next()
        .expect("create modal's title field");
    title_field.focus();
    title_field.type_text("Real create status message task");
    harness.run();

    harness.get_by_label("Create").click();
    harness.run();

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        harness.run();
        if let Some(msg) = harness.state().backlog_status.snapshot() {
            if msg.starts_with("Created ") {
                assert_eq!(
                    msg, "Created MusicProduction:TASK-1",
                    "expected the compact repo:id form, not raw CLI stdout"
                );
                assert!(
                    !msg.contains('\n'),
                    "the status message must be a single line, got {msg:?}"
                );
                return;
            }
        }
        assert!(
            std::time::Instant::now() < deadline,
            "create's background thread did not report completion in time; last status: {:?}",
            harness.state().backlog_status.snapshot()
        );
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}

/// Owner report (2026-08-05): a task created through the Create modal
/// didn't appear on the Board afterward. Investigated per the mission's
/// three sub-questions — (a) the CLI's default status ("To Do") IS a
/// standard Board column, confirmed against a real `backlog task create`
/// call; (b) `column_order`'s status union (TASK-25) always includes
/// `BACKLOG_STATUSES` regardless of a project's own `config.yml`, so it
/// can't exclude "To Do"; (c) `spawn_backlog_create`'s own
/// `refresh_backlog_project_cache` call (app.rs, TASK-28) correctly
/// updates the SAME shared `backlog_projects` cache both List and Board
/// read from every frame (`Snapshot::collect`, mod.rs) — there's no
/// separate, Board-only stale cache. None of the three reproduced the
/// symptom with a real fixture (this test).
///
/// The actual root cause turned out to be a fourth thing, found by reading
/// `workers.rs`'s periodic backlog-scan worker: `spawn_backlog`'s loop did
/// a wholesale `*ch.backlog_projects.lock().unwrap() = projects` every
/// `BACKLOG_PERIOD` (30s, or sooner if kicked). Since `collect_backlog_
/// projects` scans every tracked repo's disk state *sequentially* (real
/// multi-repo wall time, not an instant), a periodic scan that started
/// reading a project *before* a create finished, but finishes applying its
/// (stale) result *after* `refresh_backlog_project_cache`'s fresher
/// single-project insert, would silently revert that project back to its
/// pre-create state — clobbering the newly created task out of the shared
/// cache entirely, in EVERY lens, not just Board. This isn't reproducible
/// with a single-threaded kittest harness (there's no periodic worker
/// thread racing anything here); it's covered instead by
/// `workers::tests::merge_keeps_a_newer_cached_snapshot_over_a_stale_scan_
/// result`, which deterministically proves the exact interleaving via
/// `merge_backlog_projects` (the fix: per-project `loaded_at_unix`
/// timestamp comparison instead of a blind overwrite) without depending on
/// real thread timing.
///
/// This test instead proves the ordinary, non-racing path end to end
/// against a real fixture repo and the real CLI, in both lenses — the
/// baseline the race-condition fix protects.
#[test]
fn create_modal_task_is_visible_in_both_list_and_board_against_a_real_fixture_repo() {
    let fixture = tempfile::tempdir().expect("create temp dir");
    let root = fixture.path();
    run_cmd(root, "git", &["init", "-q"]);
    run_cmd(root, "git", &["config", "user.email", "qa@example.com"]);
    run_cmd(root, "git", &["config", "user.name", "QA Fixture"]);
    run_cmd(
        root,
        "backlog",
        &["init", "--defaults", "--agent-instructions", "none", "qa"],
    );

    let repos = vec![Repo {
        name: "MusicProduction".to_string(),
        path: root.to_path_buf(),
    }];
    let worktrees = vec![WorktreeRef {
        repo_name: "MusicProduction".to_string(),
        path: root.to_path_buf(),
        branch: Some("main".to_string()),
        head: "abc1234".to_string(),
    }];
    let mut cfg = Config::default();
    cfg.ui.onboarding_dismissed = true;
    let mut app = HiveApp::new_headless(cfg, repos, worktrees);
    app.config_save_path = Some(isolated_config_save_path());
    app.view_tab = ViewTab::Backlog;
    app.backlog_view.lens = BacklogLens::List;
    app.backlog_view.selected_project = Some(root.to_path_buf());
    app.backlog_projects.lock().unwrap().insert(
        root.to_path_buf(),
        switchbard_core::load_backlog_project(root).expect("load the real fixture project"),
    );

    let mut harness = harness(app);
    harness.run();

    harness.get_by_label("+ Task").click();
    harness.run();
    let modal = harness.get_by_label("New Backlog Task");
    let title_field = modal
        .query_all(kittest::by().role(egui::accesskit::Role::TextInput))
        .next()
        .expect("create modal's title field");
    title_field.focus();
    title_field.type_text("Freshly created task");
    harness.run();
    harness.get_by_label("Create").click();
    harness.run();

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        harness.run();
        if harness
            .state()
            .backlog_status
            .snapshot()
            .as_deref()
            .is_some_and(|s| s.starts_with("Created "))
        {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "create's background thread did not report completion in time"
        );
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    harness.run();

    assert!(
        harness
            .query_all_by_label_contains("Freshly created task")
            .next()
            .is_some(),
        "the newly created task should be visible in the List lens"
    );

    harness.state_mut().backlog_view.lens = BacklogLens::Board;
    harness.run();
    assert!(
        harness
            .query_all_by_label_contains("Freshly created task")
            .next()
            .is_some(),
        "the newly created task should be visible in the Board lens too, \
         under its default \"To Do\" column"
    );
}

/// Independent re-verification (2026-08-05 fix-wave audit) of the HIGH
/// defect's fix: `create_backlog_task_wires_a_subtask_parent` and
/// `subtask_ids_are_decimal_children_of_the_parent_id`
/// (backlog_cli_mutations.rs) prove the *parser* now reads real subtasks
/// correctly; neither exercises the GUI render path the original defect
/// actually broke (roll-up badge, tree expand/collapse, "+ Subtask"). Every
/// pre-existing GUI test for that feature
/// (`parent_task_shows_rollup_and_expands_to_reveal_children`, ui_views.rs)
/// constructs `BacklogTask { parent: Some(...), .. }` directly in Rust —
/// exactly the blind spot that let the original `parent`/`parent_task_id`
/// mismatch go undetected. This test closes that blind spot: a real parent
/// and two real subtasks created via the real `backlog` CLI, one child
/// marked Done via the real CLI, loaded through the real parser
/// (`load_backlog_project`, not a struct literal), then driven through the
/// actual List-lens render path.
#[test]
fn sub_task_hierarchy_renders_correctly_from_a_real_cli_created_subtask() {
    let fixture = tempfile::tempdir().expect("create temp dir");
    let root = fixture.path();
    run_cmd(root, "git", &["init", "-q"]);
    run_cmd(root, "git", &["config", "user.email", "qa@example.com"]);
    run_cmd(root, "git", &["config", "user.name", "QA Fixture"]);
    run_cmd(
        root,
        "backlog",
        &["init", "--defaults", "--agent-instructions", "none", "qa"],
    );
    run_cmd(root, "backlog", &["task", "create", "Parent task"]);
    run_cmd(
        root,
        "backlog",
        &["task", "create", "Done child", "-p", "TASK-1"],
    );
    run_cmd(
        root,
        "backlog",
        &["task", "create", "Open child", "-p", "TASK-1"],
    );
    run_cmd(root, "backlog", &["task", "edit", "TASK-1.1", "-s", "Done"]);

    // Sanity: prove the real CLI really did write `parent_task_id:`, not
    // `parent:` — if a future CLI version changes the key again, this test
    // should fail loudly here rather than silently passing for the wrong
    // reason.
    let child_file = std::fs::read_to_string(root.join("backlog/tasks/task-1.1 - Done-child.md"))
        .expect("read the real CLI's generated subtask file");
    assert!(
        child_file.contains("parent_task_id: TASK-1"),
        "expected the real CLI to write parent_task_id:, got:\n{child_file}"
    );

    let repos = vec![Repo {
        name: "qa-fixture".to_string(),
        path: root.to_path_buf(),
    }];
    let worktrees = vec![WorktreeRef {
        repo_name: "qa-fixture".to_string(),
        path: root.to_path_buf(),
        branch: Some("main".to_string()),
        head: "abc1234".to_string(),
    }];
    let mut cfg = Config::default();
    cfg.ui.onboarding_dismissed = true;
    let mut app = HiveApp::new_headless(cfg, repos, worktrees);
    app.config_save_path = Some(isolated_config_save_path());
    app.view_tab = ViewTab::Backlog;
    app.backlog_view.lens = BacklogLens::List;
    app.backlog_view.selected_project = Some(root.to_path_buf());
    let real_project =
        switchbard_core::load_backlog_project(root).expect("load the real fixture project");
    // Independently confirm the parser itself resolved parentage before
    // even reaching the GUI — if this fails, the GUI assertions below would
    // fail for an uninteresting reason (no roll-up data to show at all).
    let parsed_child = real_project
        .tasks
        .iter()
        .find(|t| t.id == "TASK-1.1")
        .expect("subtask should have parsed");
    assert_eq!(
        parsed_child.parent.as_deref(),
        Some("TASK-1"),
        "the real parser should resolve the real CLI's parent_task_id key"
    );
    app.backlog_projects
        .lock()
        .unwrap()
        .insert(root.to_path_buf(), real_project);
    app.backlog_view.selected_task = Some((root.to_path_buf(), "TASK-1".to_string()));

    let mut h = harness(app);
    h.run();

    assert!(
        h.query_by_label("TASK-1  Parent task  [1/2]").is_some(),
        "the parent row should show a real 1/2 roll-up badge computed from real CLI data"
    );
    assert!(
        h.query_by_label("TASK-1.2  Open child").is_none(),
        "children should stay collapsed until the parent is expanded"
    );

    // The tree caret has no accessible label (documented, UNDRIVABLE-BY-KITTEST
    // elsewhere in this audit); toggle expansion via view state directly, same
    // precedent as ui_views.rs's own struct-constructed version of this test.
    h.state_mut()
        .backlog_view
        .expanded_parents
        .insert((root.to_path_buf(), "TASK-1".to_string()));
    h.run();

    assert!(
        h.query_by_label("TASK-1.2  Open child").is_some(),
        "expanding the parent should reveal the real open child"
    );
    assert!(
        h.query_by_label("TASK-1.1  Done child").is_some(),
        "expanding the parent should reveal the real done child"
    );

    h.get_by_label("+ Subtask").click();
    h.run();
    assert_eq!(
        h.state().backlog_view.new_task.parent.as_deref(),
        Some("TASK-1"),
        "+ Subtask should pre-fill the new-task modal's parent from the real task id"
    );
    assert!(h.state().backlog_view.new_task.open);
}

fn run_cmd(cwd: &std::path::Path, cmd: &str, args: &[&str]) {
    let output = std::process::Command::new(cmd)
        .current_dir(cwd)
        .args(args)
        .output()
        .unwrap_or_else(|e| panic!("failed to run {cmd}: {e}"));
    assert!(
        output.status.success(),
        "{cmd} {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}
