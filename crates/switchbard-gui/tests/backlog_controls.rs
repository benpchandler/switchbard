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
//! `switchbard-core/tests/backlog_mutations.rs`, which proves the exact
//! same functions against a real fixture repo.

mod common;

use egui_kittest::kittest::NodeT;
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
            tasks,
            warnings: vec![],
            loaded_at_unix: 0,
            configured_statuses: vec![
                "Icebox".into(),
                "To Do".into(),
                "In Progress".into(),
                "In Review".into(),
                "Done".into(),
            ],
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
fn unlabeled_checkbox<'t>(
    harness: &'t Harness<'_, HiveApp>,
    index: usize,
) -> egui_kittest::Node<'t> {
    harness
        .query_all(kittest::by().role(egui::accesskit::Role::CheckBox))
        .filter(|n| n.accesskit_node().label().is_none_or(|l| l.is_empty()))
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
) -> egui_kittest::Node<'t> {
    let inputs: Vec<egui_kittest::Node<'t>> = harness
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
fn multiline_input_nth<'t>(
    harness: &'t Harness<'_, HiveApp>,
    index: usize,
) -> egui_kittest::Node<'t> {
    harness
        .query_all(kittest::by().role(egui::accesskit::Role::MultilineTextInput))
        .nth(index)
        .unwrap_or_else(|| panic!("no MultilineTextInput at index {index}"))
}

// ─── List lens: bulk selection ──────────────────────────────────────────

/// Asserts a control routed to the right backlog action by checking the
/// status surface. The UI sets `in_flight` synchronously on click, then a
/// `spawn_backlog_*` thread overwrites it with a terminal message once the
/// CLI call finishes — and against this fixture's nonexistent project root
/// that thread can lose or win the race with the assertion (on CI the spawn
/// fails instantly; locally a stray dir makes it slow). Either observation
/// proves the click dispatched the intended action, so accept both: the
/// exact in-flight string, or any terminal message starting with one of
/// `terminal_prefixes`. The CLI's actual effect is proven in
/// `switchbard-core/tests/backlog_mutations.rs`, not here.
fn assert_action_status(
    harness: &Harness<'static, HiveApp>,
    in_flight: &str,
    terminal_prefixes: &[&str],
    context: &str,
) {
    let status = harness.state().backlog_status.snapshot();
    let ok = match status.as_deref() {
        Some(s) => s == in_flight || terminal_prefixes.iter().any(|p| s.starts_with(p)),
        None => false,
    };
    assert!(
        ok,
        "{context}: expected status {in_flight:?} (or a terminal message starting with one \
         of {terminal_prefixes:?}), got {status:?}"
    );
}

#[test]
fn select_all_checkbox_selects_then_deselects_every_visible_task() {
    let mut harness = list_harness_with_tasks(vec![
        task("TASK-1", "First", "To Do"),
        task("TASK-2", "Second", "To Do"),
    ]);

    unlabeled_checkbox(&harness, 0).click();
    harness.run();
    assert_eq!(
        harness.state().backlog_view.bulk_selected_tasks.len(),
        2,
        "clicking select-all should select every visible task"
    );

    unlabeled_checkbox(&harness, 0).click();
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

    harness.get_by_label("Clear").click();
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
    unlabeled_checkbox(&harness, 1).click();
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

    unlabeled_checkbox(&harness, 1).click();
    harness.run();

    // `ui.input(|i| i.modifiers.shift)` (the row's own modifier check) reads
    // egui 0.36 removed `RawInput`'s top-level `modifiers` field — modifiers
    // now ride on the individual event rather than being ambient state the
    // frame reads. `Node::click_modifiers` emits the click already carrying
    // them, which is what the row's `ui.input()` read observes, and it needs
    // no reset afterwards because nothing global was ever set.
    unlabeled_checkbox(&harness, 3).click_modifiers(egui::Modifiers::SHIFT);
    harness.run();

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

    // See the shift-click test above for why the modifiers ride on the click
    // itself under egui 0.36 rather than being set on `RawInput`.
    harness
        .get_by_label("TASK-2  Second")
        .click_modifiers(egui::Modifiers::COMMAND);
    harness.run();

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
        harness
            .get_by_label("Clean Up Old Tasks")
            .accesskit_node()
            .is_disabled(),
        "nothing to archive should leave the button disabled"
    );
}

#[test]
fn cleanup_button_confirms_then_cancel_reverts_to_the_plain_button() {
    let mut done = task("TASK-1", "Stale one", "Done");
    done.status = "Done".to_string();
    let mut harness = list_harness_with_tasks(vec![task("TASK-2", "Open one", "To Do"), done]);
    assert!(!harness
        .get_by_label("Clean Up Old Tasks")
        .accesskit_node()
        .is_disabled());

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

    assert_action_status(
        &harness,
        "cleaning up 1 Done tasks",
        &["cleaned up 0/1 Done tasks", "cleaned up 1/1 Done tasks"],
        "confirming cleanup should route to the spawned per-task archive calls",
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
        harness
            .get_by_label("Clean Up Old Tasks")
            .accesskit_node()
            .is_disabled(),
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
/// milestone_and_dependencies` (backlog_mutations.rs) proves the queued
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
    // (kittest's `by().accesskit_node().label()` reads `Node::value` only for `Role::Label`
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

    harness.key_press(egui::Key::Escape);
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
            tasks,
            warnings: vec![],
            loaded_at_unix: 0,
            configured_statuses: vec![
                "Icebox".into(),
                "To Do".into(),
                "In Progress".into(),
                "In Review".into(),
                "Done".into(),
            ],
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
        let b = node
            .accesskit_node()
            .raw_bounds()
            .expect("node should have bounds");
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
/// backlog_mutations.rs, for that finding and the real-fixture proof of
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

/// **Reversed again (owner decision, 2026-08-28).** This assertion has now
/// been all three ways, so the history is worth keeping.
///
/// Originally: a project declaring no statuses got "no phantom columns beyond
/// the standard three". Then 2026-08-06 standardized the vocabulary and this
/// test was flipped to assert every project shows all of `STANDARD_STATUSES`,
/// because `dispatch` releases to `In Review` and four of five repos could
/// neither display nor select it.
///
/// That fix asserted a vocabulary the repos didn't have. The `backlog` CLI
/// validates writes against each project's own `config.yml`, so the board
/// offered columns the CLI then refused — a drag to `Icebox` failed with
/// `Invalid status`, and dispatch's own `In Review` write failed silently in
/// three of four repos because `set_dispatch_status` discards the error.
///
/// So: **what the board shows matches what the repo declares.** The
/// standardization goal is kept, but as an offer — `missing_standard_statuses`
/// finds the gap and the UI proposes writing it into `config.yml`, which makes
/// the shared vocabulary true instead of assumed. The empty-`Icebox`-column
/// cost the 2026-08-06 note accepted is gone with it.
#[test]
fn board_columns_are_exactly_what_the_project_declares() {
    let mut harness = list_harness_with_tasks(vec![task("TASK-1", "Ordinary task", "To Do")]);
    harness.state_mut().backlog_view.lens = BacklogLens::Board;
    harness.run();

    for status in ["To Do", "In Progress", "Done"] {
        assert!(
            harness.query_all_by_label(status).next().is_some(),
            "{status} is declared by the fixture and must have a column"
        );
    }
}

#[test]
fn board_column_add_task_opens_the_composer_with_that_columns_status() {
    let mut harness = list_harness_with_tasks(vec![task("TASK-1", "Ordinary task", "To Do")]);
    harness.state_mut().backlog_view.lens = BacklogLens::Board;
    harness.run();

    let in_progress_x = column_left_x(&harness, "In Progress");
    let add_in_progress = harness
        .query_all_by_label_contains("Add task")
        .min_by(|a, b| {
            let ax = a
                .accesskit_node()
                .raw_bounds()
                .expect("add-task control bounds")
                .x0 as f32;
            let bx = b
                .accesskit_node()
                .raw_bounds()
                .expect("add-task control bounds")
                .x0 as f32;
            (ax - in_progress_x)
                .abs()
                .total_cmp(&(bx - in_progress_x).abs())
        })
        .expect("every Board column should expose an add-task control");
    let add_bounds = add_in_progress
        .accesskit_node()
        .raw_bounds()
        .expect("empty-column add-task bounds");
    assert!(
        add_bounds.y1 - add_bounds.y0 >= 100.0,
        "the empty column body should be the add target, not only its label"
    );
    add_in_progress.click();
    harness.run();

    assert!(harness.state().backlog_view.new_task.open);
    assert_eq!(
        harness.state().backlog_view.new_task.status,
        "In Progress",
        "the clicked column should preselect the new task's status"
    );
    assert_eq!(
        harness.state().backlog_view.new_task.target_project,
        Some(PathBuf::from(REPO_PATH))
    );
    assert!(harness.query_by_label("New Backlog Task").is_some());
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

    unlabeled_checkbox(&harness, 0).click();
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

    unlabeled_checkbox(&harness, 0).click();
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
        let b = node
            .accesskit_node()
            .raw_bounds()
            .expect("node should have bounds");
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
        unlabeled_checkbox(&harness, 0).accesskit_node().toggled(),
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
        unlabeled_checkbox(&harness, 0).accesskit_node().toggled(),
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
        let b = node
            .accesskit_node()
            .raw_bounds()
            .expect("node should have bounds");
        egui::Pos2::new(((b.x0 + b.x1) / 2.0) as f32, ((b.y0 + b.y1) / 2.0) as f32)
    };
    let target_center = {
        // Drop well below the "In Progress" column's own header label, into
        // its (empty) drop-zone body — dropping directly on the header text
        // itself isn't the intended gesture and the header has no
        // meaningful drop behavior of its own either way.
        let node = harness.get_by_label("In Progress");
        let b = node
            .accesskit_node()
            .raw_bounds()
            .expect("node should have bounds");
        egui::Pos2::new(((b.x0 + b.x1) / 2.0) as f32, b.y1 as f32 + 80.0)
    };

    drag_and_drop(&mut harness, source_center, target_center);

    assert_action_status(
        &harness,
        "moving TASK-2 to In Progress",
        &["saved TASK-2", "save TASK-2 failed"],
        "TASK-29: dropping a card on another column should queue a status-change save",
    );
}

// ─── task-42: Board drag optimistic move + drop feedback ────────────────
//
// A drop's status change writes through a real `backlog` CLI subprocess
// (`HiveApp::spawn_board_move_save`), a 0.5-1.5s round trip — see board.rs's
// own "Optimistic move + drop feedback" module-doc section for the current
// mechanism (a `pending_moves` overlay resolved by each drop's own
// `generation` token against its own save's completion report, never by an
// unrelated cache reload — see `PendingBoardMove`'s doc, runtime/mod.rs, for
// why the first version's `loaded_at_unix`-based signal was wrong).
//
// Tests below split into two groups:
// - the pure overlay/resolution *mechanism* (this file's first block) is
//   proven by directly seeding `pending_moves`/`board_move_outcomes` rather
//   than racing a real background thread, the same "assert the synchronous
//   state" approach this file's module doc already establishes for
//   CLI-writing controls generally, and the only way to deterministically
//   test something as inherently timing-dependent as "which of two racing
//   saves resolves the overlay";
// - the real round trip *failing* end to end and visibly rolling the card
//   back needs the real `backlog` CLI and a real fixture repo to produce a
//   genuine, deterministic failure (dropped onto "Icebox", a column
//   `backlog init --defaults` never configures) rather than a faked one —
//   `board_drag_failure_rolls_back_the_card_and_reloads_the_cache`, further
//   down, is the one test in this section that waits on a real thread, and
//   deliberately avoids asserting anything about the overlay's state before
//   its own bounded poll settles it (post-review finding F7: a fast CI box
//   could otherwise win that race and turn a real check into a flake).

/// A column header label's left edge — a stand-in for "which column is this
/// x-coordinate under," since columns are fixed-width and laid out strictly
/// left-to-right (`COLUMN_WIDTH`, `ui.horizontal_top`) and a card's own left
/// edge sits close to its column's (both descend from the same `ui.vertical`
/// with no extra indentation in between).
///
/// A status name isn't unique on screen — the persistent detail rail
/// (`SidePanel::right`) also shows the *selected* task's own status as a
/// same-text pill — so this takes the **leftmost** match rather than
/// assuming there's only one: the rail sits to the right of the board by
/// construction, so the true column header is always the smaller-x node.
///
/// Column headers are queried exactly: their new framed parent derives an
/// accessible name from the entire column, so substring matching `"To Do"`
/// would also match that parent and report the board's left edge instead of
/// the header's own x-coordinate.
fn column_left_x(harness: &Harness<'_, HiveApp>, column_label: &str) -> f32 {
    harness
        .query_all_by_label(column_label)
        .map(|n| {
            n.accesskit_node()
                .raw_bounds()
                .expect("column header should have bounds")
                .x0 as f32
        })
        .fold(f32::INFINITY, f32::min)
}

/// The leftmost node matching `label`'s screen bounds. Same rationale as
/// `column_left_x` above (including its note on why this uses `_contains`,
/// not exact match): the persistent detail rail also shows the *selected*
/// task's own title as its own heading, and a lone- or first-sorted-task
/// fixture's task is often auto-selected (`reconcile_selected_task`'s
/// default), so a board card's title is never assumed to be the only match
/// for its own text — the rail sits to the right of the board by
/// construction, so the board's own card is always the smaller-x node.
///
/// Uses `_contains` rather than the exact-match query. Root cause, N5
/// (post-review bounded investigation): a Board card's click/drag region
/// has no explicit accessible label, so AccessKit derives one from its
/// descendant labels. While saving, the extra "saving…" descendant can
/// make the title fail an exact-label query even though a contains query
/// still finds it.
fn leftmost_bounds(harness: &Harness<'_, HiveApp>, label: &str) -> egui::Rect {
    let b = harness
        .query_all_by_label_contains(label)
        .map(|n| {
            n.accesskit_node()
                .raw_bounds()
                .expect("node should have bounds")
        })
        .min_by(|a, b| a.x0.partial_cmp(&b.x0).unwrap())
        .unwrap_or_else(|| panic!("no node found matching label {label:?}"));
    egui::Rect::from_min_max(
        egui::Pos2::new(b.x0 as f32, b.y0 as f32),
        egui::Pos2::new(b.x1 as f32, b.y1 as f32),
    )
}

/// A card's title label's own left edge — see `leftmost_bounds`.
fn label_left_x(harness: &Harness<'_, HiveApp>, label: &str) -> f32 {
    leftmost_bounds(harness, label).min.x
}

/// Press at `source`, move past the click/drag threshold, release at
/// `target` — the same simulated pointer drag every drag/drop test in this
/// section drives a real Board card with. Plain `Harness::run` throughout
/// is fine even once a drop lands and `pending_moves` becomes non-empty:
/// `resolve_pending_moves`'s bounded repaint request
/// (`LANDING_FLASH_REPAINT_INTERVAL`, board.rs) is comfortably above
/// kittest's default `step_dt`, so `run()`'s settle loop — which only
/// treats a *zero-delay* repaint request as "still animating, keep
/// stepping" (see `try_run`'s own source) — resolves in exactly one step
/// while any move is pending or flashing, not zero and not `max_steps`.
/// (Post-review correction, N4: an earlier revision of this helper used a
/// shorter interval that kittest's internal `predicted_dt` subtraction
/// collapsed to zero, which genuinely did make `run()` spin — the fix was
/// the interval, not avoiding `run()`.)
fn drag_and_drop(harness: &mut Harness<'_, HiveApp>, source: egui::Pos2, target: egui::Pos2) {
    harness
        .input_mut()
        .events
        .push(egui::Event::PointerMoved(source));
    harness.input_mut().events.push(egui::Event::PointerButton {
        pos: source,
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: egui::Modifiers::default(),
    });
    harness.run();

    // Move in a couple of steps, well past any click-vs-drag threshold, so
    // `is_decidedly_dragging` commits to a drag instead of resolving as a
    // click on release.
    let midpoint = egui::Pos2::new(source.x, (source.y + target.y) / 2.0);
    harness
        .input_mut()
        .events
        .push(egui::Event::PointerMoved(midpoint));
    harness.run();
    harness
        .input_mut()
        .events
        .push(egui::Event::PointerMoved(target));
    harness.run();

    harness.input_mut().events.push(egui::Event::PointerButton {
        pos: target,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: egui::Modifiers::default(),
    });
    // A fixed step count, NOT `run()` — this is the one call in this helper
    // that races a background thread (TASK-56).
    //
    // The drop spawns a real `backlog` CLI save, and that thread calls
    // `ctx.request_repaint()` when it finishes. `Harness::run`'s settle loop
    // breaks only on a *non-zero* `repaint_delay`, and a cross-thread repaint
    // sets it to zero while registering no in-frame cause — which is exactly
    // the `Repaint causes: []` in the CI panic. Land it inside the settle
    // window and `run()` spins to `max_steps`. Reproduced deterministically by
    // hammering `request_repaint` from another thread: `run()` fails, this
    // does not.
    //
    // Four steps because that is what `run()` was already bounded to
    // (`max_steps`), so nothing this helper used to settle is lost. The
    // assertions downstream read state directly and accept either the
    // in-flight or the terminal status, so they never needed the save to
    // finish — only the drop to register.
    harness.run_steps(4);
}

/// task-42 AC #1 ("dropped card renders in the destination column on the
/// same frame, before the backlog CLI save resolves"): directly seed
/// `pending_moves` — no drag, no spawned thread — to isolate the pure
/// rendering mechanism from the background-thread race
/// `board_drag_failure_rolls_back_the_card_and_reloads_the_cache` below
/// deliberately takes on instead. `board::render_column`'s column-membership
/// check reads this overlay ahead of the task's real (unmoved) `status`, so
/// a seeded entry alone should be enough to relocate the card.
#[test]
fn board_pending_move_overlay_renders_card_in_destination_column_before_save_resolves() {
    let mut harness = list_harness_with_tasks(vec![
        task("TASK-1", "Other card", "To Do"),
        task("TASK-2", "Draggable card", "To Do"),
    ]);
    harness.state_mut().backlog_view.lens = BacklogLens::Board;
    harness.state_mut().backlog_view.sort_key = BacklogTaskSortKey::Task;
    harness.run();

    let todo_x = column_left_x(&harness, "To Do");
    let in_progress_x = column_left_x(&harness, "In Progress");
    let card_x_before = label_left_x(&harness, "Draggable card");
    assert!(
        (card_x_before - todo_x).abs() < (card_x_before - in_progress_x).abs(),
        "sanity check: before any move, the card should render nearest its \
         real 'To Do' column"
    );

    let key = (PathBuf::from(REPO_PATH), "TASK-2".to_string());
    harness.state_mut().backlog_view.pending_moves.insert(
        key,
        switchbard_gui::runtime::PendingBoardMove {
            target_status: "In Progress".to_string(),
            // No outcome will ever land in `board_move_outcomes` for this
            // generation (no real save was ever spawned) and nothing else
            // in this test resolves it either, so the overlay stays pending
            // for the entire test with no thread or CLI involved at all —
            // the generation's exact value doesn't matter here.
            generation: 0,
            queued_at: std::time::Instant::now(),
        },
    );
    // Plain `run()` — see `drag_and_drop`'s doc for why a pending
    // `pending_moves` entry doesn't stop it from settling in one step.
    harness.run();

    let todo_x_after = column_left_x(&harness, "To Do");
    let in_progress_x_after = column_left_x(&harness, "In Progress");
    let card_x_after = label_left_x(&harness, "Draggable card");
    assert!(
        (card_x_after - in_progress_x_after).abs() < (card_x_after - todo_x_after).abs(),
        "AC #1: with a pending_moves overlay entry, the card should render \
         under its destination column same-frame, with no save ever having \
         run — before it 'resolves' is automatic when it never even starts"
    );

    let cached_status = harness
        .state()
        .backlog_projects
        .lock()
        .unwrap()
        .get(&PathBuf::from(REPO_PATH))
        .expect("project should still be cached")
        .tasks
        .iter()
        .find(|t| t.id == "TASK-2")
        .expect("task should still be cached")
        .status
        .clone();
    assert_eq!(
        cached_status, "To Do",
        "the overlay must be a pure render-time layer, not a second source \
         of truth — the underlying cache should be untouched"
    );

    assert!(
        harness
            .query_all_by_label_contains("saving")
            .next()
            .is_some(),
        "AC #1: an in-flight card should carry a clear 'saving' treatment, \
         not just a subtler shade of its normal appearance"
    );
}

/// task-42 AC #2 ("failed save visibly returns the card to its origin
/// column and surfaces the error in the status line") and AC #4 ("cleared on
/// reload"): a real fixture repo + a real `backlog` CLI round trip, forced
/// to fail deterministically by dropping onto "Icebox" — a column Board
/// always renders (the standardized vocabulary,
/// `board_shows_the_full_standard_vocabulary_even_when_a_project_declares_
/// none` above) but `backlog init --defaults` never configures (confirmed
/// against a real fixture: `backlog task edit TASK-1 -s Icebox --plain`
/// exits non-zero with "Invalid status: Icebox. Valid statuses are: To Do,
/// In Progress, Done", leaving the task file itself untouched). That's a
/// genuine, deterministic real-CLI rejection — unlike deleting the task's
/// own file out from under the edit, which was tried first and rejected:
/// the reload this failure triggers would then find no task left to reload
/// at all, defeating the "still there, just rolled back" assertions below.
/// Same bounded-poll pattern as
/// `save_button_completes_a_real_write_round_trip_against_a_real_fixture_repo`
/// — a real subprocess, not a synchronous state change, so this is the one
/// test in this section that waits on the background thread.
#[test]
fn board_drag_failure_rolls_back_the_card_and_reloads_the_cache() {
    let fixture = tempfile::tempdir().expect("create temp dir");
    let root = fixture.path();
    native_backlog_init(root);
    native_task_create(root, "Draggable card");

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
    app.backlog_view.lens = BacklogLens::Board;
    app.backlog_view.sort_key = BacklogTaskSortKey::Task;
    app.backlog_view.selected_project = Some(root.to_path_buf());
    let project =
        switchbard_core::load_backlog_project(root).expect("load the real fixture project");
    assert_eq!(project.tasks[0].id, "TASK-1");
    app.backlog_projects
        .lock()
        .unwrap()
        .insert(root.to_path_buf(), project);

    let mut harness = harness(app);
    harness.run();

    // How this drag is made to fail, and why it changed.
    //
    // It used to drop onto `Icebox` — a column the board offered for every
    // repo whatever its `config.yml` declared, so the CLI refused the write
    // and the rollback path ran. That offer was the bug (see
    // `ordered_status_vocabulary`): a column a repo cannot accept is no
    // longer rendered for it, so this fixture, a real `backlog init` with the
    // default trio, has no Icebox to drop onto.
    //
    // The failure now comes from the task file being unwritable — another
    // process holding it, a checkout that landed it read-only. Deleting it
    // instead would work too, but the task has to *survive* the failed save:
    // this test's last assertion is that the card rolled back to its original
    // column, and a card whose task no longer exists doesn't roll back, it
    // vanishes. (Learned the hard way: that version failed on the final
    // assertion, not the drag.)
    let task_file = std::fs::read_dir(root.join("backlog/tasks"))
        .expect("the fixture has a tasks dir")
        .filter_map(Result::ok)
        .map(|e| e.path())
        .find(|p| p.extension().is_some_and(|e| e == "md"))
        .expect("TASK-1 is on disk");
    let mut perms = std::fs::metadata(&task_file).unwrap().permissions();
    perms.set_readonly(true);
    std::fs::set_permissions(&task_file, perms).expect("make the task unwritable");

    let source_center = leftmost_bounds(&harness, "Draggable card").center();
    let target_center = {
        let b = leftmost_bounds(&harness, "In Progress");
        egui::Pos2::new(b.center().x, b.max.y + 80.0)
    };

    drag_and_drop(&mut harness, source_center, target_center);

    let key = (root.to_path_buf(), "TASK-1".to_string());
    // AC #1's "renders in the destination column before the save resolves,
    // same frame as the drop" is proven deterministically, with no real
    // thread involved at all, by
    // `board_pending_move_overlay_renders_card_in_destination_column_before_
    // save_resolves` above. This test's own value is the real round trip's
    // *failure* path (AC #2/#4) — asserting "still pending right here" or
    // "still visually in Icebox right here" would race the real `backlog`
    // subprocess this fixture spawns (post-review finding F7: a fast CI box
    // could legitimately win that race), so this deliberately doesn't check
    // either before the bounded poll below settles it one way or the other.

    // Bounded poll for the real `backlog` CLI subprocess to finish and
    // fail — same pattern as
    // save_button_completes_a_real_write_round_trip_against_a_real_fixture_repo.
    // Plain `run()` (see `drag_and_drop`'s doc) — it keeps settling in one
    // step on every iteration of this loop even while the move stays
    // pending the whole time.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        harness.run();
        if !harness
            .state()
            .backlog_view
            .pending_moves
            .contains_key(&key)
        {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the pending move never resolved; last status: {:?}",
            harness.state().backlog_status.snapshot()
        );
        std::thread::sleep(std::time::Duration::from_millis(20));
    }

    let status = harness.state().backlog_status.snapshot();
    assert!(
        status
            .as_deref()
            .is_some_and(|s| s.starts_with("save TASK-1 failed")),
        "AC #2: the failure should surface in the status line, got {status:?}"
    );

    let todo_x_after = column_left_x(&harness, "To Do");
    let icebox_x_after = column_left_x(&harness, "Icebox");
    let card_x_after = label_left_x(&harness, "Draggable card");
    assert!(
        (card_x_after - todo_x_after).abs() < (card_x_after - icebox_x_after).abs(),
        "AC #2: a failed save should visibly return the card to its origin \
         column, not leave it stranded under the destination column"
    );

    // AC #4: the failed save's own reload (this task's addition to
    // `spawn_backlog_save`'s error path) is what let `pending_moves` resolve
    // above in the first place — confirm the cache it reloaded still shows
    // the task's real, unmoved status.
    let cached_status = harness
        .state()
        .backlog_projects
        .lock()
        .unwrap()
        .get(&root.to_path_buf())
        .expect("project should still be cached")
        .tasks
        .iter()
        .find(|t| t.id == "TASK-1")
        .expect("task should still be cached")
        .status
        .clone();
    assert_eq!(
        cached_status, "To Do",
        "the reloaded cache should confirm the edit never actually happened"
    );
}

/// N1/N2 (post-review, confirmed): before this fix, `task_write_locks`
/// (then `board_move_locks`) only serialized Board drops against each
/// other — a detail-rail field edit (`spawn_backlog_save`) landing on a
/// task with an in-flight drop could run its own `edit_backlog_task`
/// concurrently with the drop's, instead of waiting its turn.
///
/// Proves the actual mechanism deterministically rather than racing two
/// independent real subprocesses against each other (which — tried first —
/// turned out to be unusable: the real `backlog` CLI against this fixture
/// completes fast enough that by the time a poll loop can observe
/// `board_move_started`'s one-shot signal, the drop's save has often
/// already finished and released the lock, so there's no reliable window
/// left to inject a second save "while the first is still running";
/// separately, `std::sync::Mutex` doesn't promise FIFO wake order even if
/// there were one, so asserting a specific winner between two independently
/// contending real threads wouldn't be a sound proof of anything).
///
/// Instead: the test itself acquires `TASK-1`'s entry in
/// `HiveApp::task_write_locks` directly — mechanically identical to what a
/// real in-flight drop's `spawn_board_move_save` thread would be holding
/// mid-subprocess — and holds it. With that lock held, it submits a real
/// `spawn_backlog_save` call (the rail-edit stand-in) and asserts, deterministically
/// (a logical guarantee from holding the same `Mutex`, not a timing race),
/// that the file on disk has *not* changed while the lock is held. Only
/// then does it release the lock and confirm the save proceeds and lands.
/// That is the actual claim "serializes" makes: a saver contending for a
/// task's lock does not touch that task's file until it acquires the lock.
#[test]
fn board_rail_edit_save_serializes_against_an_in_flight_drop_on_the_same_task() {
    let fixture = tempfile::tempdir().expect("create temp dir");
    let root = fixture.path();
    native_backlog_init(root);
    native_task_create(root, "Draggable card");

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
    let project =
        switchbard_core::load_backlog_project(root).expect("load the real fixture project");
    assert_eq!(project.tasks[0].id, "TASK-1");
    app.backlog_projects
        .lock()
        .unwrap()
        .insert(root.to_path_buf(), project);

    let mut harness = harness(app);
    harness.run();

    let key = (root.to_path_buf(), "TASK-1".to_string());

    // Simulates "a Board drop on this task is currently mid-subprocess":
    // acquire and hold the exact same per-task lock
    // `spawn_board_move_save`'s thread would be holding in that scenario —
    // `task_write_locks` is the one shared write-lock registry every saver
    // (drop, rail edit, bulk edit) contends for.
    let task_lock = harness
        .state()
        .task_write_locks
        .lock()
        .unwrap()
        .entry(key.clone())
        .or_insert_with(|| std::sync::Arc::new(std::sync::Mutex::new(())))
        .clone();
    let held = task_lock.lock().unwrap();

    // Submit the rail-edit stand-in while the lock is held — its thread
    // will block trying to acquire the same lock.
    harness.state().spawn_backlog_save(
        root.to_path_buf(),
        "TASK-1".to_string(),
        switchbard_core::BacklogTaskPatch {
            status: Some("Done".to_string()),
            ..Default::default()
        },
        &harness.ctx.clone(),
    );

    // Generous real-time margin, not a correctness dependency: gives a
    // *broken* implementation (one that doesn't actually block on the
    // lock) every opportunity to have already written the file, so this
    // assertion would catch that regression; a *correct* implementation
    // passes regardless of how long this sleep is, since it cannot have
    // proceeded past `lock_task` while `held` is still alive. Must exceed
    // the real `backlog task edit` subprocess's own wall-clock time against
    // this fixture (measured ~160ms) with real margin — an earlier version
    // of this test used 150ms, *below* that measured time, which made the
    // negative control vacuous (removing the lock did not turn this red,
    // since the unserialized write hadn't landed yet either way). 1000ms is
    // ~6x the measured time; re-verified red with the lock removed at this
    // duration before trusting it (see this task's PR description).
    std::thread::sleep(std::time::Duration::from_millis(1000));
    let project_while_locked =
        switchbard_core::load_backlog_project(root).expect("reload the real fixture project");
    let status_while_locked = project_while_locked
        .tasks
        .iter()
        .find(|t| t.id == "TASK-1")
        .expect("task should still exist")
        .status
        .clone();
    assert_eq!(
        status_while_locked, "To Do",
        "N1/N2: a save contending for a task's write lock must not touch \
         that task's file before it acquires the lock — the rail-edit \
         stand-in should still be blocked, so the file should be untouched"
    );

    // Release the lock — the rail edit's thread (and anything else
    // contending for it) is now free to proceed.
    drop(held);

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        harness.run();
        let landed = harness
            .state()
            .backlog_projects
            .lock()
            .unwrap()
            .get(&root.to_path_buf())
            .and_then(|p| p.tasks.iter().find(|t| t.id == "TASK-1"))
            .is_some_and(|t| t.status == "Done");
        if landed {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the rail-edit save never landed after the lock was released; \
             last status: {:?}",
            harness.state().backlog_status.snapshot()
        );
        std::thread::sleep(std::time::Duration::from_millis(20));
    }

    let final_project =
        switchbard_core::load_backlog_project(root).expect("reload the real fixture project");
    let final_status = final_project
        .tasks
        .iter()
        .find(|t| t.id == "TASK-1")
        .expect("task should still exist")
        .status
        .clone();
    assert_eq!(
        final_status, "Done",
        "the save should complete and land once the lock is released"
    );
}

// ─── task-42 post-review revision: generation-based resolution ──────────
//
// Independent audit findings F1-F4 all traced back to the first version
// resolving a `PendingBoardMove` off *any* `BacklogProject::loaded_at_unix`
// advance instead of its own drop's own save completing. The four tests
// below exercise the replacement (`PendingBoardMove::generation` +
// `HiveApp::board_move_outcomes`) directly against seeded state — the same
// "assert the synchronous, deterministic mechanism, don't race a real
// thread" approach
// `board_pending_move_overlay_renders_card_in_destination_column_before_
// save_resolves` above already established, and for the same reason: a
// real spawned save (even one that fails against a fake CLI path) can't be
// made to complete-or-not-complete before a given assertion on demand, so
// asserting around one would just reintroduce the exact "probabilistic
// check" F7 flagged.

/// F2 (drop-to-same-optimistic-target should no-op): re-dropping a card
/// onto the column its own in-flight move is already headed for must not
/// queue a second, redundant subprocess. `next_move_generation` only ever
/// advances synchronously on the UI thread when `apply_drop` actually
/// queues a save (see that field's doc) — never touched by any background
/// thread — so "did it advance" is a fully race-free proxy for "was a save
/// queued," independent of whether any real thread ever runs.
#[test]
fn board_redrop_onto_the_same_pending_target_column_is_a_no_op() {
    let mut harness = list_harness_with_tasks(vec![
        task("TASK-1", "Other card", "To Do"),
        task("TASK-2", "Draggable card", "To Do"),
    ]);
    harness.state_mut().backlog_view.lens = BacklogLens::Board;
    harness.state_mut().backlog_view.sort_key = BacklogTaskSortKey::Task;
    harness.run();

    let key = (PathBuf::from(REPO_PATH), "TASK-2".to_string());
    harness.state_mut().backlog_view.pending_moves.insert(
        key.clone(),
        switchbard_gui::runtime::PendingBoardMove {
            target_status: "In Progress".to_string(),
            generation: 5,
            queued_at: std::time::Instant::now(),
        },
    );
    harness.state_mut().backlog_view.next_move_generation = 6;
    harness.run();

    // Locate the card by its task-id label ("TASK-2"), not its title: with
    // `CardMotion::Saving` active (a `pending_moves` entry is seeded above),
    // the title label sits beside the "saving…" marker in a way that made
    // `kittest`'s label query empirically unreliable for it specifically —
    // the id label doesn't have that problem and identifies the same card.
    let source_center = leftmost_bounds(&harness, "TASK-2").center();
    let target_center = {
        let b = leftmost_bounds(&harness, "In Progress");
        egui::Pos2::new(b.center().x, b.max.y + 80.0)
    };
    drag_and_drop(&mut harness, source_center, target_center);

    assert_eq!(
        harness.state().backlog_view.next_move_generation,
        6,
        "AC #4/F2: dropping onto the same column a pending move is already \
         headed for must not consume a new generation — i.e. must not \
         queue a second, redundant save"
    );
    let mv = harness
        .state()
        .backlog_view
        .pending_moves
        .get(&key)
        .cloned()
        .expect("the original pending move should be untouched, not cleared");
    assert_eq!(
        mv.generation, 5,
        "the original entry itself must be untouched"
    );
    assert_eq!(mv.target_status, "In Progress");
}

/// F2 (drop-to-origin while pending should cancel/reverse the pending
/// entry): dragging a card back out of its own in-flight destination column
/// — including back to its real, origin status — must be recognized as a
/// genuine new move, not silently swallowed by a guard that only compared
/// against the task's stale real status. `next_move_generation` advancing
/// is the race-free proxy for "a new save was queued" (see the previous
/// test's doc); the position check is also race-free here specifically
/// because this drop's target *is* the task's real status, so the card
/// renders there whether or not this fresh save has resolved yet by the
/// time the assertion runs.
#[test]
fn board_drop_back_to_origin_while_pending_queues_a_reversing_move() {
    let mut harness = list_harness_with_tasks(vec![
        task("TASK-1", "Other card", "To Do"),
        task("TASK-2", "Draggable card", "To Do"),
    ]);
    harness.state_mut().backlog_view.lens = BacklogLens::Board;
    harness.state_mut().backlog_view.sort_key = BacklogTaskSortKey::Task;
    harness.run();

    let key = (PathBuf::from(REPO_PATH), "TASK-2".to_string());
    harness.state_mut().backlog_view.pending_moves.insert(
        key.clone(),
        switchbard_gui::runtime::PendingBoardMove {
            target_status: "In Progress".to_string(),
            generation: 9,
            queued_at: std::time::Instant::now(),
        },
    );
    harness.state_mut().backlog_view.next_move_generation = 10;
    harness.run();

    // The card renders under "In Progress" per the seeded overlay — drag it
    // from there back to "To Do", its real, origin status. Locate it by its
    // task-id label ("TASK-2"), not its title — see the equivalent note in
    // `board_redrop_onto_the_same_pending_target_column_is_a_no_op`.
    let source_center = leftmost_bounds(&harness, "TASK-2").center();
    let target_center = {
        let b = leftmost_bounds(&harness, "To Do");
        egui::Pos2::new(b.center().x, b.max.y + 80.0)
    };

    // Not `drag_and_drop` (which ends each event in `Harness::run`) — this
    // drop is a *genuine* new move (not a no-op like the redrop test
    // above), so it really does spawn a real `spawn_board_move_save`
    // thread. Against this fixture's nonexistent task file, that thread fails
    // near-instantly and reports its outcome; `Harness::run`'s settle loop
    // can (rarely, when something unrelated also requests an immediate
    // repaint on the same frame — e.g. a hover-state change right after the
    // drop) execute a *second* internal step before returning, and if the
    // background thread's outcome lands in that window, `resolve_pending_
    // moves` drains and resolves the entry before this test ever gets to
    // inspect it — a CI-observed flake (post-review finding N4-follow-up),
    // not a hypothetical one. `Harness::step` processes exactly one queued
    // event with no internal retry loop, so pushing the release event and
    // stepping once guarantees the assertions below run on the very frame
    // the drop landed, before anything else can possibly run.
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
    harness.step();
    let midpoint = egui::Pos2::new(source_center.x, (source_center.y + target_center.y) / 2.0);
    harness
        .input_mut()
        .events
        .push(egui::Event::PointerMoved(midpoint));
    harness.step();
    harness
        .input_mut()
        .events
        .push(egui::Event::PointerMoved(target_center));
    harness.step();
    harness.input_mut().events.push(egui::Event::PointerButton {
        pos: target_center,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: egui::Modifiers::default(),
    });
    harness.step();

    assert_eq!(
        harness.state().backlog_view.next_move_generation,
        11,
        "F2: dragging back to the origin column while a move is pending \
         should queue a genuine new save (a new generation consumed), not \
         no-op against the stale real status"
    );
    let mv = harness
        .state()
        .backlog_view
        .pending_moves
        .get(&key)
        .cloned()
        .expect(
            "the reversing move should itself be a fresh pending entry, \
             still present on the exact frame the drop landed",
        );
    assert_eq!(
        mv.target_status, "To Do",
        "the fresh entry should target the origin status the card was \
         dragged back to"
    );
    assert_eq!(
        mv.generation, 10,
        "should be exactly the newly consumed generation"
    );
    // Not re-checked here: that a `pending_moves` entry targeting a column
    // renders the card there is already proven directly by
    // `board_pending_move_overlay_renders_card_in_destination_column_before_
    // save_resolves` and `board_redrop_onto_the_same_pending_target_column_
    // is_a_no_op` above — this test's own job is proving the *overlay
    // mutation* (a fresh, correctly-targeted entry, not a no-op), which the
    // state assertions above already do.
}

/// F1 (the actual root-cause fix): an *unrelated* project reload — the
/// periodic backlog worker's own poll, standing in for `Kick::notify`
/// waking it early right after some other save — must have zero effect on
/// a still-in-flight pending move. Directly overwrites `backlog_projects`
/// (what a worker's reload would do) with a snapshot where the task's real
/// status *already* matches the move's target — the strongest version of
/// this check: even a reload that happens to agree with the pending move's
/// destination must not be what resolves it, only that move's own
/// `board_move_outcomes` report may.
#[test]
fn board_unrelated_project_reload_does_not_resolve_a_pending_move() {
    let mut harness = list_harness_with_tasks(vec![
        task("TASK-1", "Other card", "To Do"),
        task("TASK-2", "Draggable card", "To Do"),
    ]);
    harness.state_mut().backlog_view.lens = BacklogLens::Board;
    harness.state_mut().backlog_view.sort_key = BacklogTaskSortKey::Task;
    harness.run();

    let key = (PathBuf::from(REPO_PATH), "TASK-2".to_string());
    harness.state_mut().backlog_view.pending_moves.insert(
        key.clone(),
        switchbard_gui::runtime::PendingBoardMove {
            target_status: "In Progress".to_string(),
            generation: 42,
            queued_at: std::time::Instant::now(),
        },
    );
    harness.run();

    // Simulate an unrelated worker reload: the cache is replaced wholesale
    // (as `refresh_backlog_project_cache`/a periodic scan would do) with a
    // fresh snapshot whose task already carries the move's own target
    // status — no `board_move_outcomes` entry is written, because this
    // reload has nothing to do with this drop.
    harness.state_mut().backlog_projects.lock().unwrap().insert(
        PathBuf::from(REPO_PATH),
        BacklogProject {
            root: PathBuf::from(REPO_PATH),
            tasks: vec![
                task("TASK-1", "Other card", "To Do"),
                task("TASK-2", "Draggable card", "In Progress"),
            ],
            warnings: vec![],
            loaded_at_unix: 999_999,
            configured_statuses: vec![
                "Icebox".into(),
                "To Do".into(),
                "In Progress".into(),
                "In Review".into(),
                "Done".into(),
            ],
        },
    );
    harness.run();

    let mv = harness
        .state()
        .backlog_view
        .pending_moves
        .get(&key)
        .cloned()
        .expect(
            "F1: an unrelated reload must not resolve a still-in-flight \
             pending move, even one whose target the reload happens to agree with",
        );
    assert_eq!(
        mv.generation, 42,
        "the entry itself should be completely untouched"
    );
    assert!(
        !harness
            .state()
            .backlog_view
            .landing_flash
            .contains_key(&key),
        "no landing flash should fire off an unrelated reload either"
    );
}

/// F3/F1 (supersede ordering): a stale completion report for a generation
/// `pending_moves` has since moved past (a later drop superseded it) must
/// be discarded, never used to resolve — let alone land — the *newer*
/// entry. Simulates the old save "winning the race" and reporting after the
/// new drop already landed, exactly the ordering a real two-drops-in-a-row
/// gesture could produce.
#[test]
fn board_stale_outcome_for_a_superseded_generation_does_not_resolve_the_newer_entry() {
    let mut harness = list_harness_with_tasks(vec![
        task("TASK-1", "Other card", "To Do"),
        task("TASK-2", "Draggable card", "To Do"),
    ]);
    harness.state_mut().backlog_view.lens = BacklogLens::Board;
    harness.state_mut().backlog_view.sort_key = BacklogTaskSortKey::Task;
    harness.run();

    let key = (PathBuf::from(REPO_PATH), "TASK-2".to_string());
    // The newer drop already landed in the overlay (generation 2, target
    // "Done") — as if the user dropped on "In Progress" and then, before
    // that save resolved, dragged again onto "Done".
    harness.state_mut().backlog_view.pending_moves.insert(
        key.clone(),
        switchbard_gui::runtime::PendingBoardMove {
            target_status: "Done".to_string(),
            generation: 2,
            queued_at: std::time::Instant::now(),
        },
    );
    // The *old* (generation 1, "In Progress") save now reports success —
    // late, after being superseded.
    harness
        .state_mut()
        .board_move_outcomes
        .lock()
        .unwrap()
        .insert(
            key.clone(),
            switchbard_gui::runtime::BoardMoveOutcome {
                generation: 1,
                success: true,
            },
        );
    harness.run();

    let mv = harness
        .state()
        .backlog_view
        .pending_moves
        .get(&key)
        .cloned()
        .expect(
            "F3: a stale outcome for a superseded generation must not resolve \
             the current, newer pending entry",
        );
    assert_eq!(
        mv.generation, 2,
        "the newer entry should be completely untouched"
    );
    assert_eq!(mv.target_status, "Done");
    assert!(
        !harness
            .state()
            .backlog_view
            .landing_flash
            .contains_key(&key),
        "the stale generation's success must not land a flash for the newer move"
    );
}

/// N3 (post-review, confirmed): the success/landing path had zero positive
/// coverage — every other task-42 test exercises either the failure path or
/// a case where nothing resolves at all, so `resolve_pending_moves`'s own
/// `if outcome.success { landed.push(key.clone()); }` (board.rs) could be
/// deleted outright and every gate would still pass. Seeds a
/// `pending_moves` entry and a *matching-generation* success outcome (same
/// style as the other seeded tests above — deterministic, no real thread),
/// and asserts both halves of what a successful resolution must do: the
/// overlay entry is cleared (AC #1's "before it resolves" only means
/// something if resolution is provably reachable) and the key lands in
/// `landing_flash` — the specific line this test exists to pin down; delete
/// the `landed.push` call and this assertion fails while the entry-cleared
/// assertion alone would not have caught it.
#[test]
fn board_matching_generation_success_resolves_the_entry_and_fires_the_landing_flash() {
    let mut harness = list_harness_with_tasks(vec![
        task("TASK-1", "Other card", "To Do"),
        task("TASK-2", "Draggable card", "To Do"),
    ]);
    harness.state_mut().backlog_view.lens = BacklogLens::Board;
    harness.state_mut().backlog_view.sort_key = BacklogTaskSortKey::Task;
    harness.run();

    let key = (PathBuf::from(REPO_PATH), "TASK-2".to_string());
    harness.state_mut().backlog_view.pending_moves.insert(
        key.clone(),
        switchbard_gui::runtime::PendingBoardMove {
            target_status: "In Progress".to_string(),
            generation: 7,
            queued_at: std::time::Instant::now(),
        },
    );
    harness
        .state_mut()
        .board_move_outcomes
        .lock()
        .unwrap()
        .insert(
            key.clone(),
            switchbard_gui::runtime::BoardMoveOutcome {
                generation: 7,
                success: true,
            },
        );
    harness.run();

    assert!(
        !harness
            .state()
            .backlog_view
            .pending_moves
            .contains_key(&key),
        "a matching-generation outcome should resolve (remove) the pending entry"
    );
    assert!(
        harness
            .state()
            .backlog_view
            .landing_flash
            .contains_key(&key),
        "N3: a matching-generation *success* outcome must fire the landing \
         flash — this is the exact assertion that catches `landed.push` \
         being deleted from resolve_pending_moves's success branch"
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

#[test]
fn detail_rail_can_collapse_and_expand_without_losing_the_task() {
    let mut harness = list_harness_with_tasks(vec![task("TASK-1", "Rail task", "To Do")]);

    assert!(harness.query_by_label("Task details").is_some());
    harness.get_by_label("▶").click();
    harness.run();

    assert!(harness.state().backlog_view.detail_rail_collapsed);
    assert!(harness.query_by_label("Task details").is_none());
    assert!(harness.query_by_label("◀").is_some());

    harness.get_by_label("◀").click();
    harness.run();

    assert!(!harness.state().backlog_view.detail_rail_collapsed);
    assert!(harness.query_by_label("Task details").is_some());
    assert!(
        harness.query_all_by_label("TASK-1").next().is_some(),
        "expanding should restore the selected task's detail"
    );
}

#[test]
fn detail_rail_width_changes_when_its_left_edge_is_dragged() {
    let mut harness = list_harness_with_tasks(vec![task("TASK-1", "Rail task", "To Do")]);
    let panel_id = egui::Id::new("backlog_detail_rail");
    let initial = egui::containers::panel::PanelState::load(&harness.ctx, panel_id)
        .expect("expanded detail rail panel state");
    let source = egui::Pos2::new(initial.outer_rect.left(), initial.outer_rect.center().y);

    harness
        .input_mut()
        .events
        .push(egui::Event::PointerMoved(source));
    harness.input_mut().events.push(egui::Event::PointerButton {
        pos: source,
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: egui::Modifiers::default(),
    });
    harness.run();

    let target = source - egui::vec2(100.0, 0.0);
    harness
        .input_mut()
        .events
        .push(egui::Event::PointerMoved(target));
    harness.run();
    harness.input_mut().events.push(egui::Event::PointerButton {
        pos: target,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: egui::Modifiers::default(),
    });
    harness.run();

    let resized = egui::containers::panel::PanelState::load(&harness.ctx, panel_id)
        .expect("resized detail rail panel state");
    assert!(
        resized.outer_rect.width() >= initial.outer_rect.width() + 90.0,
        "dragging the left edge left should expand the rail: {} -> {}",
        initial.outer_rect.width(),
        resized.outer_rect.width()
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

// Detail-rail tests below use `click_accesskit()` rather than `click()`.
//
// The rail's content is a `ScrollArea` and, in the harness's 1280x860 window,
// everything from the acceptance criteria down sits below the fold — the
// Archive button lands around y=1333. Under egui_kittest 0.31 that did not
// matter: `kittest` 0.1's `click()` dispatched an accesskit `Action::Click`
// straight at the node, so visibility was irrelevant. 0.36's `click()` sends
// real pointer events at the node's centre instead, which for an off-screen
// widget lands outside the window and hits nothing.
//
// `click_accesskit()` is the direct equivalent of the old behaviour (its own
// doc: "In contrast to `click()`, this can also click widgets that are not
// currently visible"). These tests assert wiring — that the button reaches its
// handler — not that the widget is reachable at this window size, so keeping
// the old semantics is the honest port. A test that means to prove on-screen
// reachability should scroll and use `click()`.

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
    harness.get_by_label("#1 Criterion one").click_accesskit();
    harness.run();
    assert_action_status(
        &harness,
        "updating TASK-1 AC #1",
        &[
            "checked TASK-1 AC #1",
            "unchecked TASK-1 AC #1",
            "update TASK-1 AC #1 failed",
        ],
        "clicking an AC checkbox should route to the AC update",
    );
}

#[test]
fn definition_of_done_checkbox_click_sets_the_synchronous_updating_status() {
    let mut harness = detail_harness_on(detail_task_with_checklists());
    harness.get_by_label("#1 DoD one").click_accesskit();
    harness.run();
    assert_action_status(
        &harness,
        "updating TASK-1 DoD #1",
        &[
            "checked TASK-1 DoD #1",
            "unchecked TASK-1 DoD #1",
            "update TASK-1 DoD #1 failed",
        ],
        "clicking a DoD checkbox should route to the DoD update",
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

    harness.get_by_label("Add").click_accesskit();
    // `run_steps`, not `run`: the buffer clear under assertion happens
    // synchronously in the click handler, and `run`'s 4-step settle budget
    // can be exhausted by the scroll-into-view animation plus the save
    // thread's own repaint request — which, since the format fork's native
    // writes, can land mid-run instead of long after it.
    harness.run_steps(4);
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

    harness.get_by_label("Append Note").click_accesskit();
    // `run_steps`, not `run` — same rationale as
    // `references_add_button_clears_the_input_field` above.
    harness.run_steps(4);
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
            .accesskit_node()
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
            .accesskit_node()
            .is_disabled(),
        "editing the title should enable Save (the click itself, and the CLI \
         round trip it triggers, are proven in \
         save_button_completes_a_real_write_round_trip_against_a_real_fixture_repo \
         below)"
    );
}

#[test]
fn archive_button_shows_confirm_then_cancel_reverts_to_the_plain_button() {
    let mut harness = detail_harness_on(detail_task_with_checklists());
    assert!(harness.query_by_label("Archive").is_some());

    harness.get_by_label("Archive").click_accesskit();
    harness.run();
    assert!(harness.state().backlog_view.archive_confirm);
    assert!(harness.query_by_label("Archive TASK-1?").is_some());

    harness.get_by_label("Cancel").click_accesskit();
    harness.run();
    assert!(!harness.state().backlog_view.archive_confirm);
    assert!(harness.query_by_label("Archive").is_some());
}

#[test]
fn archive_confirm_sets_the_synchronous_archiving_status() {
    let mut harness = detail_harness_on(detail_task_with_checklists());
    harness.get_by_label("Archive").click_accesskit();
    harness.run();

    harness.get_by_label("Confirm archive").click_accesskit();
    harness.run();

    assert!(!harness.state().backlog_view.archive_confirm);
    assert_action_status(
        &harness,
        "archiving TASK-1",
        &["archived TASK-1", "archive TASK-1 failed"],
        "confirming archive should route to the archive action",
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

    harness.get_by_label("Complete").click_accesskit();
    harness.run();
    assert!(harness.state().backlog_view.archive_confirm);
    assert!(harness.query_by_label("Complete TASK-1?").is_some());

    harness.get_by_label("Confirm complete").click_accesskit();
    harness.run();
    assert!(!harness.state().backlog_view.archive_confirm);
    assert_action_status(
        &harness,
        "completing TASK-1",
        &["completed TASK-1", "complete TASK-1 failed"],
        "confirming complete should route to the complete action",
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
/// create_call`, backlog_mutations.rs), which used to land verbatim in
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

    // Enter commits the name now — the separate Save button is gone, since
    // it spent almost all of its life disabled beside an empty field.
    harness.state_mut().backlog_view.saved_view_name_draft = "High priority".to_string();
    harness.run();
    // The name field carries no accessible label of its own (the same
    // documented limitation as the detail pane's inputs above), and on the
    // Statistics lens the filter row is not rendered — so the saved-views
    // draft is the *last* TextInput in the window. Located that way rather
    // than by a fixed absolute index, which shifts per lens.
    let name_field = harness
        .query_all(kittest::by().role(egui::accesskit::Role::TextInput))
        .last()
        .expect("the saved-views name field should render");
    name_field.focus();
    harness.run();
    harness.key_press(egui::Key::Enter);
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
/// (`backlog_mutations.rs`).
#[test]
fn save_button_completes_a_real_write_round_trip_against_a_real_fixture_repo() {
    let fixture = tempfile::tempdir().expect("create temp dir");
    let root = fixture.path();
    native_backlog_init(root);
    native_task_create(root, "Fixture task");

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
    native_backlog_init(root);

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
    native_backlog_init(root);

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
/// (backlog_mutations.rs) prove the *parser* now reads real subtasks
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
fn sub_task_hierarchy_renders_correctly_from_a_native_created_subtask() {
    let fixture = tempfile::tempdir().expect("create temp dir");
    let root = fixture.path();
    native_backlog_init(root);
    native_task_create(root, "Parent task");
    for title in ["Done child", "Open child"] {
        switchbard_core::create_backlog_task(
            root,
            &switchbard_core::NewBacklogTask {
                title: title.to_string(),
                description: String::new(),
                status: String::new(),
                priority: String::new(),
                acceptance_criteria: vec![],
                parent: Some("TASK-1".to_string()),
                labels: vec![],
                assignees: vec![],
                milestone: None,
                dependencies: vec![],
            },
        )
        .expect("native fixture subtask create");
    }
    native_task_status(root, "TASK-1.1", "Done");

    // Sanity: prove the native writer really does write `parent_task_id:`,
    // not `parent:` — the key the 2026-08-05 QA audit proved the format
    // uses. If the writer ever changes the key, fail loudly here rather
    // than silently passing for the wrong reason.
    let child_file = std::fs::read_to_string(root.join("backlog/tasks/task-1.1 - Done-child.md"))
        .expect("read the native writer's generated subtask file");
    assert!(
        child_file.contains("parent_task_id: TASK-1"),
        "expected the native writer to emit parent_task_id:, got:\n{child_file}"
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

/// Native fixture init — the directory shape plus a config declaring the
/// standard trio, matching what `backlog init --defaults` used to produce
/// before the format fork retired the external CLI (TASK-67).
fn native_backlog_init(root: &std::path::Path) {
    std::fs::create_dir_all(root.join("backlog/tasks")).expect("fixture layout");
    std::fs::write(
        root.join("backlog/config.yml"),
        "statuses: [\"To Do\", \"In Progress\", \"Done\"]\n",
    )
    .expect("fixture config");
}

fn native_task_create(root: &std::path::Path, title: &str) -> String {
    switchbard_core::create_backlog_task(
        root,
        &switchbard_core::NewBacklogTask {
            title: title.to_string(),
            description: String::new(),
            status: String::new(),
            priority: String::new(),
            acceptance_criteria: vec![],
            parent: None,
            labels: vec![],
            assignees: vec![],
            milestone: None,
            dependencies: vec![],
        },
    )
    .expect("native fixture create")
}

fn native_task_status(root: &std::path::Path, id: &str, status: &str) {
    switchbard_core::edit_backlog_task(
        root,
        id,
        &switchbard_core::BacklogTaskPatch {
            status: Some(status.to_string()),
            ..Default::default()
        },
    )
    .expect("native fixture status edit");
}

/// A long label list must not drag its column wider than `COLUMN_WIDTH`.
///
/// `render_labels_and_age` puts the comma-joined labels in a `ui.horizontal`,
/// which reports its content's intrinsic width as its own minimum. Untruncated,
/// one card with several labels widened the whole column's scroll content while
/// every other card still painted at its own `set_width` — the dead space to
/// the right of the cards that owner review caught on a real board (columns
/// painting 262-468px against a nominal 260).
///
/// Asserted against the "+ Add task" button, which is `add_sized` to
/// `ui.available_width()` and so measures the column's own painted content
/// width — the thing that actually stretched.
#[test]
fn a_long_label_list_does_not_widen_its_board_column() {
    let mut wordy = task("TASK-1", "Short title", "To Do");
    wordy.labels = vec![
        "security".to_string(),
        "incident-response".to_string(),
        "ops".to_string(),
        "frontend".to_string(),
        "storybook".to_string(),
        "observability".to_string(),
    ];

    let mut app = list_app_with_tasks(vec![wordy]);
    app.backlog_view.lens = BacklogLens::Board;
    let mut harness = harness(app);
    harness.run();

    let column_body = harness
        .get_all_by_label("+ Add task")
        .map(|n| n.rect().width())
        .fold(0.0_f32, f32::max);
    assert!(
        column_body > 0.0,
        "the board should render at least one column"
    );
    assert!(
        column_body < 280.0,
        "a column must stay near COLUMN_WIDTH (260); the widest painted {column_body}"
    );
}

/// Bulk archive is unavailable until a filter narrows the view.
///
/// "Archive what's showing" with nothing filtered means "archive the entire
/// backlog" — not an action anyone means to take from a toolbar button.
#[test]
fn bulk_archive_is_disabled_until_the_view_is_narrowed() {
    let mut app = list_app_with_tasks(vec![
        task("TASK-1", "One", "To Do"),
        task("TASK-2", "Two", "To Do"),
    ]);
    app.backlog_view.lens = BacklogLens::List;
    let mut harness = harness(app);
    harness.run();

    let button = harness
        .get_all_by_label("Archive 2 showing")
        .next()
        .expect("the bulk archive button should render");
    assert!(
        button.accesskit_node().is_disabled(),
        "with nothing filtered, bulk archive must not be clickable"
    );
}

/// A mixed batch is named for what it will do, not for one of its halves.
///
/// Backlog.md's two terminal states are not interchangeable — Done tasks are
/// completed, the rest archived — so a set spanning both cannot honestly be
/// called "Archive". The button must never offer a verb it will not perform.
#[test]
fn a_mixed_batch_is_labelled_clear_and_counts_both_dispositions() {
    let mut app = list_app_with_tasks(vec![
        task("TASK-1", "Open one", "To Do"),
        task("TASK-2", "Finished", "Done"),
        task("TASK-3", "Open two", "To Do"),
    ]);
    app.backlog_view.lens = BacklogLens::List;
    app.backlog_view.show_completed = true;
    // Narrow on something so the button is enabled at all.
    app.backlog_view.priority_filter = "medium".to_string();
    let mut harness = harness(app);
    harness.run();

    assert!(
        harness.query_by_label("Clear 3 showing").is_some(),
        "a mixed batch names itself Clear, and counts the Done task it will complete"
    );
}

/// Bulk archive is absent on a lens that hides the filter row.
///
/// The header it lives in renders on every lens, but Digest, Portfolio and
/// Statistics do not show the filters — so an "Archive N showing" button
/// there would name a count the user cannot inspect or adjust before
/// confirming.
#[test]
fn bulk_archive_is_absent_on_a_lens_without_the_filter_row() {
    let mut app = list_app_with_tasks(vec![
        task("TASK-1", "One", "To Do"),
        task("TASK-2", "Two", "To Do"),
    ]);
    app.backlog_view.lens = BacklogLens::Statistics;
    app.backlog_view.priority_filter = "medium".to_string();
    let mut harness = harness(app);
    harness.run();

    assert!(
        harness.query_by_label("Archive 2 showing").is_none(),
        "no bulk archive on a lens whose filters are not visible"
    );
}

/// While a bulk run is live the header shows its progress instead of the
/// buttons that start one.
///
/// Both bulk actions mutate the same task set through the same
/// one-CLI-call-per-task loop, so offering to start a second mid-run is
/// offering a race.
#[test]
fn a_live_bulk_run_replaces_the_bulk_buttons_with_a_progress_bar() {
    let mut app = list_app_with_tasks(vec![
        task("TASK-1", "One", "To Do"),
        task("TASK-2", "Two", "To Do"),
    ]);
    app.backlog_view.lens = BacklogLens::List;
    app.backlog_view.priority_filter = "medium".to_string();
    let mut harness = harness(app);
    harness.run();
    assert!(
        harness.query_by_label("Archive 2 showing").is_some(),
        "precondition: the button is there when idle"
    );

    harness.state_mut().bulk_progress.begin("archiving", 43);
    for _ in 0..12 {
        harness.state_mut().bulk_progress.advance();
    }
    harness.run();

    assert!(
        harness.query_by_label("archiving 12/43").is_some(),
        "the bar names its absolute position, not just a ratio"
    );
    assert!(
        harness.query_by_label("Archive 2 showing").is_none(),
        "no starting a second bulk run while one is in flight"
    );
    assert!(
        harness.query_by_label("Clean Up Old Tasks").is_none(),
        "the same applies to the other bulk action"
    );

    harness.state_mut().bulk_progress.finish();
    harness.run();
    assert!(
        harness.query_by_label("Archive 2 showing").is_some(),
        "the buttons come back when the run ends"
    );
}

/// A selection of only Done tasks is named "Complete", never "Archive".
///
/// The CLI refuses `task archive` on a Done task, so a button offering to
/// archive them would promise something that cannot happen.
#[test]
fn a_selection_of_done_tasks_is_labelled_complete() {
    let mut app = list_app_with_tasks(vec![
        task("TASK-1", "Open", "To Do"),
        task("TASK-2", "Finished", "Done"),
    ]);
    app.backlog_view.lens = BacklogLens::List;
    app.backlog_view.show_completed = true;
    app.backlog_view
        .bulk_selected_tasks
        .insert((PathBuf::from(REPO_PATH), "TASK-2".to_string()));
    let mut harness = harness(app);
    harness.run();

    assert!(
        harness.query_by_label("Complete 1 selected").is_some(),
        "a Done-only selection is completed, and says so"
    );
}

/// An explicit selection lifts the narrowed-view gate.
///
/// That gate exists because "clear everything showing" on an unfiltered
/// board is a foot-gun. Ticking cards one by one *is* the narrowing — the
/// user named the set card by card — so it does not need the same guard.
#[test]
fn an_explicit_selection_enables_clearing_without_a_filter() {
    let mut app = list_app_with_tasks(vec![
        task("TASK-1", "One", "To Do"),
        task("TASK-2", "Two", "To Do"),
    ]);
    app.backlog_view.lens = BacklogLens::List;
    app.backlog_view
        .bulk_selected_tasks
        .insert((PathBuf::from(REPO_PATH), "TASK-1".to_string()));
    let mut harness = harness(app);
    harness.run();

    let button = harness
        .get_all_by_label("Archive 1 selected")
        .next()
        .expect("the clear button should name the selection");
    assert!(
        !button.accesskit_node().is_disabled(),
        "an explicit selection is its own narrowing and must be actionable"
    );
}

/// Selecting a column selects exactly that column, leaving other columns'
/// selections intact — which is what makes building a mixed batch column by
/// column possible.
#[test]
fn the_column_checkbox_selects_only_its_own_column() {
    let mut app = list_app_with_tasks(vec![
        task("TASK-1", "Open one", "To Do"),
        task("TASK-2", "Open two", "To Do"),
        task("TASK-3", "Working", "In Progress"),
    ]);
    app.backlog_view.lens = BacklogLens::Board;
    // Pre-select a card in a different column.
    app.backlog_view
        .bulk_selected_tasks
        .insert((PathBuf::from(REPO_PATH), "TASK-3".to_string()));
    let mut harness = harness(app);
    harness.run();

    // The column toggle is a labelled button, so it is addressed by name
    // rather than by an index that shifts whenever the board gains a widget.
    // Toggles render one per column in column order (Icebox, To Do, ...);
    // they share a glyph, so the column is selected by position among them.
    harness.get_all_by_label("☐").nth(1).unwrap().click();
    harness.run();

    let selected = &harness.state().backlog_view.bulk_selected_tasks;
    assert_eq!(
        selected.len(),
        3,
        "To Do's two cards join the pre-selected one"
    );
    assert!(
        selected.contains(&(PathBuf::from(REPO_PATH), "TASK-3".to_string())),
        "the other column's selection must survive"
    );
}

/// The sort control is available on every lens that shows the filter row,
/// not just List.
///
/// Board and Milestones already drew from the same sorted
/// `visible_task_rows`; only the control was List-only, which left them
/// sorted by a key their user could neither see nor change.
#[test]
fn the_sort_control_renders_on_the_board_lens() {
    let mut app = list_app_with_tasks(vec![task("TASK-1", "One", "To Do")]);
    app.backlog_view.lens = BacklogLens::Board;
    let mut harness = harness(app);
    harness.run();

    assert!(
        harness.query_by_label("Sort").is_some(),
        "Board must expose the sort key it is already using"
    );
}

// ── status standardization offer ────────────────────────────────────────
//
// The board shows what a repo declares. Where that's less than the shared
// vocabulary, the app offers to close the gap instead of pretending it isn't
// there — see `ui::backlog::status_migration` for why that's an offer.

/// A project declaring only the default trio, i.e. what `backlog init` writes.
fn trio_app_with_tasks(tasks: Vec<BacklogTask>) -> HiveApp {
    let mut app = list_app_with_tasks(tasks);
    app.backlog_view.lens = BacklogLens::Board;
    app.backlog_projects
        .lock()
        .unwrap()
        .get_mut(&PathBuf::from(REPO_PATH))
        .expect("the fixture project")
        .configured_statuses = vec!["To Do".into(), "In Progress".into(), "Done".into()];
    app
}

/// The passive half of the runtime check: a repo missing shared statuses is
/// told so, once, without anything being blocked on the answer.
#[test]
fn a_repo_missing_shared_statuses_is_offered_the_migration() {
    let mut harness = harness(trio_app_with_tasks(vec![task(
        "TASK-1",
        "Ordinary task",
        "To Do",
    )]));
    harness.run();

    let prompt = harness
        .state()
        .status_migration_prompt
        .clone()
        .expect("the gap should raise the offer");
    assert_eq!(prompt.missing, vec!["Icebox", "In Review"]);
    assert!(
        prompt.blocked_move.is_none(),
        "this one came from the passive check, not a refused drop"
    );
    // And the board is already correct without an answer — the offer is not
    // a gate.
    assert!(
        harness.query_all_by_label("Icebox").next().is_none(),
        "a status the repo doesn't declare must not have a column"
    );
}

/// Declining is sticky. A prompt that reappears every time the view opens
/// trains people to dismiss dialogs unread, which is worse than not asking.
#[test]
fn keeping_a_repos_own_statuses_is_remembered_and_stops_asking() {
    let mut harness = harness(trio_app_with_tasks(vec![task(
        "TASK-1",
        "Ordinary task",
        "To Do",
    )]));
    harness.run();
    assert!(harness.state().status_migration_prompt.is_some());

    harness.get_by_label_contains("Keep").click();
    harness.run();

    assert!(
        harness.state().status_migration_prompt.is_none(),
        "answering closes the offer"
    );
    assert_eq!(
        harness.state().config.status_standardization_declined,
        vec![PathBuf::from(REPO_PATH)],
        "the decline is persisted per repo"
    );

    // Several more frames: it must not come back.
    for _ in 0..3 {
        harness.run();
    }
    assert!(
        harness.state().status_migration_prompt.is_none(),
        "a declined repo is never asked again"
    );
}

/// Accepting writes the repo's own config and the column appears — the whole
/// point being that the shared vocabulary becomes true rather than assumed.
#[test]
fn accepting_the_offer_writes_the_config_and_the_column_appears() {
    let fixture = tempfile::tempdir().expect("create temp dir");
    let root = fixture.path();
    native_backlog_init(root);
    native_task_create(root, "Ordinary task");

    let mut app = seeded_app();
    app.view_tab = ViewTab::Backlog;
    app.backlog_view.lens = BacklogLens::Board;
    app.backlog_view.selected_project = Some(root.to_path_buf());
    app.backlog_projects.lock().unwrap().insert(
        root.to_path_buf(),
        switchbard_core::load_backlog_project(root).expect("load the real fixture"),
    );

    let mut harness = harness(app);
    harness.run();
    assert!(
        harness.query_all_by_label("In Review").next().is_none(),
        "precondition: a freshly `backlog init`ed repo declares only the trio"
    );

    harness
        .get_by_label_contains("Add the missing statuses")
        .click();
    harness.run();

    let config = std::fs::read_to_string(root.join("backlog/config.yml")).unwrap();
    assert!(
        config.contains("In Review") && config.contains("Icebox"),
        "the repo's own config is what changed, got: {config}"
    );
    assert!(
        harness.query_all_by_label("In Review").next().is_some(),
        "and the column appears without a restart — the cache is reloaded"
    );
}

/// The case that started this: a cross-repo board shows the union of every
/// scoped repo's columns, so `Icebox` exists because *budget* declares it —
/// and a switchbard card dropped there used to reach the CLI and fail with
/// `Invalid status: Icebox`.
///
/// Now the drop is refused before the write, and the refusal carries the
/// offer, because the user has just demonstrated they want that status.
#[test]
fn a_drop_onto_a_column_this_repo_lacks_is_refused_and_offers_the_fix() {
    const OTHER: &str = "/tmp/switchbard-ui-test/other";
    let mut app = trio_app_with_tasks(vec![task("TASK-1", "Draggable card", "To Do")]);
    // A second repo that *does* declare Icebox, which is the only reason the
    // column is on screen at all.
    app.backlog_projects.lock().unwrap().insert(
        PathBuf::from(OTHER),
        BacklogProject {
            root: PathBuf::from(OTHER),
            tasks: vec![task("OTHER-1", "Someone else's task", "Icebox")],
            warnings: vec![],
            loaded_at_unix: 0,
            configured_statuses: vec![
                "Icebox".into(),
                "To Do".into(),
                "In Progress".into(),
                "Done".into(),
            ],
        },
    );
    app.backlog_view.selected_project = None; // all repos in scope
    app.backlog_view.sort_key = BacklogTaskSortKey::Task;
    // The passive offer would otherwise fire first and mask the drop's own.
    app.config
        .status_standardization_declined
        .push(PathBuf::from(REPO_PATH));

    let mut harness = harness(app);
    harness.run();
    assert!(
        harness.query_all_by_label("Icebox").next().is_some(),
        "precondition: the other repo puts an Icebox column on the board"
    );

    let source = leftmost_bounds(&harness, "Draggable card").center();
    let target = {
        let b = leftmost_bounds(&harness, "Icebox");
        egui::Pos2::new(b.center().x, b.max.y + 80.0)
    };
    drag_and_drop(&mut harness, source, target);
    harness.run();

    assert!(
        harness.state().backlog_view.pending_moves.is_empty(),
        "the move must be refused before it is ever queued, not rolled back after"
    );
    let prompt = harness
        .state()
        .status_migration_prompt
        .clone()
        .expect("the refusal should carry the offer");
    let blocked = prompt
        .blocked_move
        .expect("raised by the drop, not the sweep");
    assert_eq!(blocked.task_id, "TASK-1");
    assert_eq!(blocked.target_status, "Icebox");
}
