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
use switchbard_gui::runtime::{BacklogLens, BacklogTaskSortDirection, ViewTab};

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

#[test]
fn search_result_row_click_navigates_to_the_task_in_the_list_lens() {
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
    assert_eq!(harness.state().backlog_view.lens, BacklogLens::List);
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

#[test]
fn digest_card_click_selects_the_task_and_jumps_to_list() {
    let mut in_progress = task("TASK-1", "Active work", "In Progress");
    in_progress.updated_date = Some("2026-08-01 12:00".to_string());
    let mut harness = digest_harness_with(vec![in_progress]);

    click_at_node_center(&mut harness, "Active work");
    harness.run();

    assert_eq!(harness.state().backlog_view.lens, BacklogLens::List);
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
// Board card click (select, and — TASK-24 — jump to the List lens's detail
// pane): UNDRIVABLE-BY-KITTEST, discovered while adding TASK-24's coverage
// during the 2026-08-05 egui 0.30->0.31 upgrade.
//
// The pre-existing `board_card_click_selects_the_task` test (since removed)
// only ever used a single-task fixture. With one task in scope,
// `reconcile_selected_task` (mod.rs) auto-selects it regardless of whether
// the click does anything, so that test passed without proving the click
// worked — it always had, even before this upgrade. A two-task
// discriminating version, added to actually isolate the click's effect,
// showed selection never moves off the auto-selected default no matter
// which board card is clicked. Confirmed exhaustively before concluding
// this rather than assuming it:
//   1. `Node::click()` (accesskit semantic action) — the toggle-selected-card
//      approach.
//   2. Position-based `PointerMoved`/`PointerButton` simulation at the
//      labeled node's center (`click_at_node_center`, used successfully for
//      Digest's structurally-identical card below) — same result.
//   3. The same position-based simulation with an extra `harness.run()`
//      inserted between the move/press/release events, in case click-vs-drag
//      resolution needed a frame boundary — same result.
//   4. A non-drag-wrapped card (an Archived-source task, so `editable` is
//      false and `render_strip` takes the plain `paint_strip` branch instead
//      of `ui.dnd_drag_source(...)`) — ruling out drag-wrapping as the
//      cause — same result.
// Digest's card uses the identical `egui::Frame::show(...).response.
// interact(Sense::click())` pattern and *does* click-drive successfully
// (see `digest_card_click_selects_the_task_and_jumps_to_list`), so the
// difference isn't the Frame-response pattern itself. The most likely cause
// is Board's nested scroll areas (an outer horizontal `ScrollArea` of
// columns, each with its own vertical one, each column also wrapped in a
// `dnd_drop_zone`) interacting with kittest's synthetic-event/clip-rect
// handling — the same general family of limitation the original QA audit
// already found for Board's own column scrolling ("Column horizontal
// scroll | UNDRIVABLE ... scroll-position simulation not exercised").
//
// Verification for both the pre-existing select-on-click behavior and
// TASK-24's List-lens jump instead rests on:
//   1. Code review: `render_strip`'s click branch (board.rs) is a five-line,
//      non-branching block — set `selected_task`, clear `editor.loaded_key`,
//      set `lens = BacklogLens::List` — reusing exactly the same three
//      assignments `digest::render_strip`'s proven-working card click makes
//      (digest.rs), which drives the identical downstream List-lens detail
//      pane. There is no board-specific branch that could plausibly behave
//      differently once the click itself lands.
//   2. `backlog_list_and_detail_{light,dark}.png` (docs/qa/screenshots/)
//      show the destination — the List lens's detail pane — rendering
//      correctly for a selected task.
//   3. `board_card_shows_labels_and_a_humanized_age` and
//      `board_card_omits_the_label_line_when_there_are_no_labels` below
//      already prove the card itself (the click's own target) renders and
//      is queryable.

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

// TASK-26 (owner-requested UX): Board bulk select + bulk-edit — UNDRIVABLE-
// BY-KITTEST for the *click* on either the per-card checkbox or the
// right-click context menu, discovered while adding this coverage.
//
// The card checkbox is a native `egui::Checkbox` — not the retroactively-
// `.interact()`ed `egui::Frame` response TASK-24's own UNDRIVABLE note is
// about — so it was a reasonable hope this one *would* click-drive even
// though the card itself doesn't. It doesn't: `unlabeled_checkbox(&harness,
// 1).simulate_click()` (the exact method `row_bulk_checkbox_click_selects_
// the_task` below already proves works for List's structurally-identical
// checkbox) leaves `bulk_selected_tasks` empty on Board. This rules out
// "retroactive Frame interact specifically" as the root cause and points at
// something common to *every* interactive element inside a column: each
// column is both a `dnd_drop_zone` and its own vertical `ScrollArea`,
// nested inside the board's own outer horizontal `ScrollArea` — the same
// nesting the original QA audit already flagged as UNDRIVABLE for column
// scrolling itself. The right-click context menu shares the List lens's own
// already-documented UNDRIVABLE status for bare-interact context menus
// (see the "List lens: right-click bulk context menu" note above), reused
// unmodified rather than reimplemented.
//
// Verification instead rests on:
//   1. Code review — the checkbox's click handler and the context menu call
//      are both under ten lines, non-branching, and route through the
//      exact same `selection::set_bulk_task_selected`/`select_bulk_task_
//      range`/`focus_context_selection` and `list::render_task_context_
//      menu` functions List's own proven-working bulk UI uses — see
//      `row_bulk_checkbox_click_selects_the_task` and
//      `shift_click_on_a_second_row_checkbox_selects_the_contiguous_range`
//      below for that proof, at the function level shared by both lenses.
//   2. The render (read) side is directly testable without a click —
//      `board_card_checkbox_reflects_bulk_selection_state` below sets
//      `bulk_selected_tasks` directly and confirms the card's own checkbox
//      renders checked, proving the binding is wired correctly in the one
//      direction that doesn't require simulating a click.
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
    // Two "Save" buttons render on this pane (the field editor's and the
    // Dependencies section's, in that order) — index 0 is the field editor.
    // Three "Save"-labeled buttons render in a List-lens harness: the
    // saved-views bar's (index 0), the field editor's (index 1), and
    // Dependencies' (index 2) — all disabled with no pending edits.
    assert!(
        harness
            .get_all_by_label("Save")
            .nth(1)
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
            .nth(1)
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
    let mut harness = list_harness_with_tasks(vec![task("TASK-1", "First", "To Do")]);
    assert!(!harness.state().config.ui.sidebar_collapsed);
    assert!(
        harness.query_by_label("Tracked repos").is_some(),
        "expanded by default: the heading should render"
    );

    harness.get_by_label("▶").click();
    harness.run();
    assert!(harness.state().config.ui.sidebar_collapsed);
    assert!(
        harness.query_by_label("Tracked repos").is_none(),
        "collapsed: the repo list content should not render"
    );
    assert!(
        harness.query_by_label("◀").is_some(),
        "collapsed: the rail should offer the expand toggle"
    );

    harness.get_by_label("◀").click();
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
    let mut harness = list_harness_with_tasks(vec![task("TASK-1", "First", "To Do")]);
    harness.get_by_label("▶").click();
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

    // Three "Save"-labeled buttons render (saved-views bar's, field
    // editor's, Dependencies'); index 1 is the field editor's.
    harness.get_all_by_label("Save").nth(1).unwrap().click();
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
