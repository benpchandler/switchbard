//! TASK-41: the bulk-remove sweep's two safety guarantees —
//!
//!   1. a dirty or unmerged worktree in the bulk selection lands in the
//!      needs-review list, never in `removable`, and the primary worktree
//!      never survives into either list even if it somehow ends up selected
//!      (`bulk_remove_confirm_auto_deselects_dirty_and_unmerged_into_needs_review`,
//!      driven through `egui_kittest`);
//!   2. actually confirming removal deletes every `removable` worktree AND
//!      its branch, verified against real `git worktree list` output
//!      (`run_bulk_removal_deletes_all_removable_worktrees_and_their_branches`,
//!      calling `worktree_actions::run_bulk_removal` directly).
//!
//! Both use real temp git repos (like `worktree_removal_orchestration.rs`)
//! because the classification and removal underneath (`collect_dirty_files`,
//! `assess_branch_delete`, `remove_worktree`, `delete_branch`) shell out to
//! real `git` — there's no fake to substitute without re-testing a parallel
//! "is this safe" check instead of the real one.
//!
//! `execute_bulk_remove_worktrees` itself (the `Arc<Mutex<>>`/`egui::Context`/
//! worker-thread wrapper around `run_bulk_removal`) is deliberately left
//! untested here, the same reasoning `worktree_removal_orchestration.rs`
//! gives for skipping `execute_remove_worktree`: threading makes end-to-end
//! orchestration testing impractical from the outside, so the decision/
//! execution logic is tested directly instead.

mod common;

use eframe::egui;
use egui_kittest::kittest::NodeT;
use std::fs;
use std::path::{Path, PathBuf};

use common::{harness, isolated_config_save_path};
use egui_kittest::kittest::Queryable;
use switchbard_core::config::Config;
use switchbard_core::dispatch_inspect::{DispatchRun, DispatchRunLiveness};
use switchbard_core::git_cmd;
use switchbard_core::{Repo, WorktreeRef};
use switchbard_gui::app::HiveApp;
use switchbard_gui::runtime::Place;

/// One repo, one primary + three linked worktrees covering the three
/// classification outcomes the bulk dialog must distinguish:
///
///   - `clean_merged` — no unique commits vs `main`, no uncommitted files.
///   - `dirty` — landed on `main` but has an uncommitted scratch file.
///   - `unmerged` — a commit not yet on `main`.
///
/// Returns `(TempDir, repo_path, clean_merged, dirty, unmerged)`.
fn setup_repo_with_three_worktrees() -> (tempfile::TempDir, PathBuf, PathBuf, PathBuf, PathBuf) {
    let tmp = tempfile::TempDir::new().unwrap();
    let repo = tmp.path().join("repo");
    fs::create_dir(&repo).unwrap();

    run(&repo, &["init", "-q", "-b", "main"]);
    run(&repo, &["config", "user.email", "test@example.com"]);
    run(&repo, &["config", "user.name", "Test"]);
    fs::write(repo.join("README.md"), "hello\n").unwrap();
    run(&repo, &["add", "."]);
    run(&repo, &["commit", "-qm", "init"]);

    let clean_merged = tmp.path().join("wt-clean-merged");
    run(
        &repo,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "feat/clean-merged",
            clean_merged.to_str().unwrap(),
        ],
    );

    let dirty = tmp.path().join("wt-dirty");
    run(
        &repo,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "feat/dirty",
            dirty.to_str().unwrap(),
        ],
    );
    fs::write(dirty.join("scratch.txt"), "uncommitted\n").unwrap();

    let unmerged = tmp.path().join("wt-unmerged");
    run(
        &repo,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "feat/unmerged",
            unmerged.to_str().unwrap(),
        ],
    );
    fs::write(unmerged.join("feature.txt"), "new work\n").unwrap();
    run(&unmerged, &["add", "."]);
    run(&unmerged, &["commit", "-qm", "unmerged work"]);

    (tmp, repo, clean_merged, dirty, unmerged)
}

fn run(cwd: &Path, args: &[&str]) {
    let status = git_cmd()
        .arg("-C")
        .arg(cwd)
        .args(args)
        .status()
        .expect("git");
    assert!(status.success(), "git {args:?} failed in {cwd:?}");
}

fn wt(repo_name: &str, path: PathBuf, branch: &str) -> WorktreeRef {
    WorktreeRef {
        repo_name: repo_name.to_string(),
        path,
        branch: Some(branch.to_string()),
        head: "abc1234".to_string(),
    }
}

fn app_with_worktrees(repo: PathBuf, worktrees: Vec<WorktreeRef>) -> HiveApp {
    let repos = vec![Repo {
        name: "demo".to_string(),
        path: repo,
    }];
    let mut cfg = Config::default();
    cfg.ui.onboarding_dismissed = true;
    // `Config` is the source of truth for repos and the runtime `repos` mutex
    // is kept in lock-step with it (see CLAUDE.md). A fixture that seeds only
    // the mutex is not a state the app can reach, and it silently disables
    // every path that resolves a repo by name — `apply_pending`'s
    // `open_remove_worktree` lookup among them, which is why the trash button
    // looked dead in tests while working in the app.
    cfg.repos = repos.clone();
    let mut app = HiveApp::new_headless(cfg, repos, worktrees);
    // MUST be set on every test-constructed HiveApp — see `common::
    // isolated_config_save_path`'s doc (this is exactly how TASK-22 happened).
    app.config_save_path = Some(isolated_config_save_path());
    // IA V2 (TASK-96): `place` now defaults to `Place::Digest`, not the old
    // `ViewTab::Servers` default this fixture used to get for free — every
    // worktree row this suite clicks lives in the Ops place's Workspace
    // body, which otherwise never renders.
    app.place = Place::Ops;
    app
}

#[test]
fn bulk_remove_confirm_auto_deselects_dirty_and_unmerged_into_needs_review() {
    let (_tmp, repo, clean_merged, dirty, unmerged) = setup_repo_with_three_worktrees();
    let worktrees = vec![
        wt("demo", repo.clone(), "main"), // primary
        wt("demo", clean_merged.clone(), "feat/clean-merged"),
        wt("demo", dirty.clone(), "feat/dirty"),
        wt("demo", unmerged.clone(), "feat/unmerged"),
    ];
    let mut app = app_with_worktrees(repo.clone(), worktrees);
    // Select every worktree, including the primary — the dialog-open path
    // must silently drop it rather than ever offering it up for removal.
    app.bulk_selected_worktrees = [
        repo.clone(),
        clean_merged.clone(),
        dirty.clone(),
        unmerged.clone(),
    ]
    .into_iter()
    .collect();

    let mut harness = harness(app);
    harness.run();

    harness.get_by_label("Remove 4 selected…").click();
    harness.run();

    let state = harness
        .state()
        .confirm_bulk_remove_worktrees
        .lock()
        .unwrap()
        .clone()
        .expect("confirming should open the bulk-remove dialog");

    assert_eq!(
        state.removable.len(),
        1,
        "only the clean, merged worktree should be removable; got {:?}",
        state.removable
    );
    assert_eq!(state.removable[0].worktree_path, clean_merged);

    assert_eq!(
        state.needs_review.len(),
        2,
        "dirty + unmerged should both be routed to needs-review; got {:?}",
        state.needs_review
    );
    let review_paths: Vec<PathBuf> = state
        .needs_review
        .iter()
        .map(|c| c.worktree_path.clone())
        .collect();
    assert!(
        review_paths.contains(&dirty),
        "dirty worktree must land in needs-review, not removable"
    );
    assert!(
        review_paths.contains(&unmerged),
        "unmerged worktree must land in needs-review, not removable"
    );

    // The primary was in the selection but must never appear in either list
    // — it is never a legal bulk-remove candidate.
    assert!(!state.removable.iter().any(|c| c.worktree_path == repo));
    assert!(!state.needs_review.iter().any(|c| c.worktree_path == repo));

    // Opening the dialog consumes the transient selection.
    assert!(
        harness.state().bulk_selected_worktrees.is_empty(),
        "the working selection should be cleared once the dialog opens"
    );
}

/// The two blockers the sweep gained when "safe to remove" was collapsed into
/// one shared definition (`switchbard_core::removal_safety`).
///
/// Both were previously invisible here. A locked worktree passed
/// classification and then failed at `git worktree remove`, because the
/// `locked` porcelain line was parsed and thrown away. A dispatched agent
/// holding the worktree passed too, because the old check counted only
/// attributed listeners and services this instance had started - and a
/// headless agent is neither.
#[test]
fn a_locked_worktree_and_a_live_dispatch_run_both_route_to_needs_review() {
    let (_tmp, repo, clean_merged, dirty, _unmerged) = setup_repo_with_three_worktrees();
    // `dirty` is clean apart from its scratch file; lock it and clear the file
    // so the *only* thing left to object to is the lock itself.
    std::fs::remove_file(dirty.join("scratch.txt")).ok();
    run(
        &repo,
        &[
            "worktree",
            "lock",
            "--reason",
            "held",
            dirty.to_str().unwrap(),
        ],
    );

    let worktrees = vec![
        wt("demo", repo.clone(), "main"),
        wt("demo", clean_merged.clone(), "feat/clean-merged"),
        wt("demo", dirty.clone(), "feat/dirty"),
    ];
    let mut app = app_with_worktrees(repo.clone(), worktrees);

    // A dispatch agent alive in the otherwise-perfect worktree.
    app.dispatch_runs.lock().unwrap().insert(
        (repo.clone(), "TASK-1".to_string()),
        DispatchRun {
            task_id: "TASK-1".to_string(),
            branch: "feat/clean-merged".to_string(),
            worktree_path: clean_merged.clone(),
            worktree_exists: true,
            log_path: None,
            prompt_path: None,
            started_at_unix: Some(1),
            log_bytes: 0,
            log_modified_unix: None,
            progress: switchbard_core::dispatch_inspect::RunProgress::default(),
            liveness: DispatchRunLiveness::Alive {
                pgid: 4242,
                supervised: true,
            },
        },
    );
    app.bulk_selected_worktrees = [clean_merged.clone(), dirty.clone()].into_iter().collect();

    let mut harness = harness(app);
    harness.run();
    harness.get_by_label("Remove 2 selected…").click();
    harness.run();

    let state = harness
        .state()
        .confirm_bulk_remove_worktrees
        .lock()
        .unwrap()
        .clone()
        .expect("confirming should open the bulk-remove dialog");

    assert!(
        state.removable.is_empty(),
        "neither worktree is safe; got removable {:?}",
        state
            .removable
            .iter()
            .map(|c| c.worktree_path.clone())
            .collect::<Vec<_>>()
    );
    assert_eq!(state.needs_review.len(), 2);

    let reason_for = |path: &PathBuf| {
        state
            .needs_review
            .iter()
            .find(|c| &c.worktree_path == path)
            .and_then(|c| c.review_reason.clone())
            .unwrap_or_default()
    };
    assert!(
        reason_for(&dirty).contains("Locked by git: held"),
        "a locked worktree must say so; got {:?}",
        reason_for(&dirty)
    );
    assert!(
        reason_for(&clean_merged).contains("1 dispatch run still running here"),
        "a live dispatch run must block the sweep; got {:?}",
        reason_for(&clean_merged)
    );
}

/// The bar has to move once per candidate, including for a candidate that
/// fails, because it measures position in the batch rather than success. A bar
/// that stalls on the first failure is worse than no bar: it says "still
/// working" about work that already stopped.
#[test]
fn progress_advances_once_per_candidate_including_failures() {
    let (_tmp, repo, clean_merged, _dirty, _unmerged) = setup_repo_with_three_worktrees();

    let real = switchbard_gui::runtime::BulkRemoveCandidate {
        repo_path: repo.clone(),
        worktree_path: clean_merged.clone(),
        display_name: "clean".into(),
        branch: Some("feat/clean-merged".into()),
        branch_assessment: None,
        review_reason: None,
    };
    // A path git has never heard of: `worktree remove` fails, and the sweep
    // must still count it as one item finished.
    let doomed = switchbard_gui::runtime::BulkRemoveCandidate {
        repo_path: repo.clone(),
        worktree_path: repo.join("does-not-exist"),
        display_name: "ghost".into(),
        branch: None,
        branch_assessment: None,
        review_reason: None,
    };

    let mut ticks = 0usize;
    let summary =
        switchbard_gui::worktree_actions::run_bulk_removal(&[real, doomed], false, || ticks += 1);

    assert_eq!(ticks, 2, "one tick per candidate, not per success");
    assert_eq!(summary.removed.len(), 1);
    assert!(
        summary.first_error.is_some(),
        "the doomed candidate should have recorded a failure"
    );
}

/// A real positional left-click at `pos`.
///
/// `Node::click()` can only click something kittest can see, so it cannot
/// express "the user clicked *there*" — needed for the trash icon below,
/// which (like `CollapsingState`'s old expand triangle) is painter-drawn
/// with a bare `ui.allocate_exact_size(..., Sense::click())` and so carries
/// no accesskit label of its own to query by.
fn click_at(harness: &egui_kittest::Harness<'static, HiveApp>, pos: egui::Pos2) {
    harness.event(egui::Event::PointerMoved(pos));
    for pressed in [true, false] {
        harness.event(egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::default(),
        });
    }
}

// TASK-100: the merged Ops table replaced the swimlane row's implicit
// "click anywhere in the header selects it" gesture with an explicit
// checkbox only — a flat `egui_extras::TableBuilder` row has no single
// pre-children-registered `Ui` to hang the old z-order trick on (each
// column is its own walled-off cell `Ui`), and a checkbox is the
// conventional, discoverable way to select a row in a dense data table
// anyway. The retired tests this file used to carry —
// `clicking_blank_row_space_selects_the_worktree`,
// `clicking_a_hover_only_label_in_the_row_selects_the_worktree`,
// `clicking_the_row_name_still_selects_the_worktree`, and the expand/
// collapse-triangle pair (`the_expand_triangle_still_toggles_without_
// selecting`, `clicking_an_expanded_rows_body_does_not_select_it`) —
// exercised gestures and a progressive-disclosure body that no longer
// exist: every row shows everything inline now, so there is nothing left
// to expand. The three below prove what still holds: the checkbox selects,
// Rename doesn't accidentally select, and the trash icon still opens its
// dialog without selecting either.

/// The row's own checkbox must toggle selection exactly once.
#[test]
fn the_row_checkbox_toggles_selection_exactly_once() {
    let (_tmp, repo, clean_merged, _dirty, _unmerged) = setup_repo_with_three_worktrees();
    let worktrees = vec![
        wt("demo", repo.clone(), "main"),
        wt("demo", clean_merged.clone(), "feat/clean-merged"),
    ];
    let mut harness = harness(app_with_worktrees(repo.clone(), worktrees));
    harness.run();

    // The view has other checkboxes (top-bar toggles), so pick the one
    // sitting on the linked row's line — anchored on the branch label
    // (`↳ feat/clean-merged`), which is what the Worktree cell renders for a
    // non-primary row now (TASK-100; the retired swimlane row anchored on
    // the worktree's directory-derived display name instead, which no
    // longer appears on this row at all). The primary row renders no
    // checkbox, being no candidate for removal.
    let row_y = harness.get_by_label("feat/clean-merged").rect().center().y;
    let boxes: Vec<_> = harness
        .get_all_by_role(egui::accesskit::Role::CheckBox)
        .filter(|n| n.rect().y_range().contains(row_y))
        .collect();
    assert_eq!(boxes.len(), 1, "exactly one checkbox on the linked row");
    boxes[0].click();
    harness.run();
    assert!(
        harness
            .state()
            .bulk_selected_worktrees
            .contains(&clean_merged),
        "the checkbox must select the row, not select-then-deselect it"
    );
}

/// Rename opens its dialog without touching selection — the row has no
/// implicit click-anywhere gesture left for it to collide with, but the
/// invariant ("Rename never selects") is still worth pinning on its own.
#[test]
fn clicking_rename_opens_its_dialog_without_selecting() {
    let (_tmp, repo, clean_merged, _dirty, _unmerged) = setup_repo_with_three_worktrees();
    let worktrees = vec![
        wt("demo", repo.clone(), "main"),
        wt("demo", clean_merged.clone(), "feat/clean-merged"),
    ];
    let mut harness = harness(app_with_worktrees(repo.clone(), worktrees));
    harness.run();

    // Both the primary and linked rows render a Rename button; take the
    // linked row's.
    let rename: Vec<_> = harness.get_all_by_label("Rename").collect();
    assert_eq!(rename.len(), 2, "primary and linked rows each have one");
    rename[1].click();
    harness.run();
    assert!(
        harness.state().rename_worktree_dialog.is_some(),
        "Rename must still open its dialog"
    );
    assert!(
        harness.state().bulk_selected_worktrees.is_empty(),
        "clicking Rename must not select the row"
    );
}

/// The trash icon opens the single-row removal dialog without selecting the
/// row. It's the one Actions-cell affordance with no accesskit label of its
/// own (see `click_at`'s doc), so it's located by position: the empty gap
/// between the linked row's Rename button and its checkbox — in that order,
/// the only thing `render_actions_cell` puts there (TASK-100).
#[test]
fn the_trash_button_opens_its_dialog_without_selecting() {
    let (_tmp, repo, clean_merged, _dirty, _unmerged) = setup_repo_with_three_worktrees();
    let worktrees = vec![
        wt("demo", repo.clone(), "main"),
        wt("demo", clean_merged.clone(), "feat/clean-merged"),
    ];
    let mut harness = harness(app_with_worktrees(repo.clone(), worktrees));
    harness.run();

    let rename = harness.get_all_by_label("Rename").nth(1).unwrap().rect();
    let row_y = rename.center().y;
    let boxes: Vec<_> = harness
        .get_all_by_role(egui::accesskit::Role::CheckBox)
        .filter(|n| n.rect().y_range().contains(row_y))
        .collect();
    let checkbox = boxes[0].rect();
    // The gap between the two named widgets, not a span across either of
    // them — Rename's own body sits to the left of this interval, the
    // checkbox's to the right, and the trash icon is the only thing between.
    let trash = egui::pos2((rename.right() + checkbox.left()) / 2.0, row_y);

    click_at(&harness, trash);
    harness.run();
    assert!(
        harness
            .state()
            .confirm_remove_worktree
            .lock()
            .unwrap()
            .is_some(),
        "the trash button must still open the removal dialog"
    );
    assert!(
        harness.state().bulk_selected_worktrees.is_empty(),
        "clicking the trash button must not select the row"
    );
}

/// The primary worktree renders no checkbox at all — it's dropped from the
/// bulk-remove candidate list (`git worktree remove` refuses it), so there
/// is nothing to select.
#[test]
fn the_primary_row_renders_no_checkbox() {
    let (_tmp, repo, clean_merged, _dirty, _unmerged) = setup_repo_with_three_worktrees();
    let worktrees = vec![
        wt("demo", repo.clone(), "main"),
        wt("demo", clean_merged.clone(), "feat/clean-merged"),
    ];
    let mut harness = harness(app_with_worktrees(repo.clone(), worktrees));
    harness.run();

    // "main" (no leading "↳") is the primary row's own branch label — see
    // `render_worktree_cell`.
    let row_y = harness.get_by_label("main").rect().center().y;
    let boxes = harness
        .get_all_by_role(egui::accesskit::Role::CheckBox)
        .filter(|n| n.rect().y_range().contains(row_y))
        .count();
    assert_eq!(boxes, 0, "the primary row must not render a checkbox");
}

/// TASK-100 removal gating: a dirty worktree's single-row remove dialog must
/// still open from the merged table's trash icon, and it must still name the
/// force required — "Discard changes and remove", not a bare "Remove
/// worktree" — and enumerate the uncommitted file. The confirm dialog's own
/// logic (`render_shared_checks`, `render_branch_delete_section`,
/// `RemovalSafety::evaluate`) is unchanged by this task; this proves the
/// *new* Actions-cell trigger still reaches it end to end.
#[test]
fn the_trash_button_on_a_dirty_row_opens_a_dialog_that_names_the_force_required() {
    let (_tmp, repo, _clean_merged, dirty, _unmerged) = setup_repo_with_three_worktrees();
    let worktrees = vec![
        wt("demo", repo.clone(), "main"),
        wt("demo", dirty.clone(), "feat/dirty"),
    ];
    let mut harness = harness(app_with_worktrees(repo.clone(), worktrees));
    harness.run();

    let rename = harness.get_all_by_label("Rename").nth(1).unwrap().rect();
    let row_y = rename.center().y;
    let boxes: Vec<_> = harness
        .get_all_by_role(egui::accesskit::Role::CheckBox)
        .filter(|n| n.rect().y_range().contains(row_y))
        .collect();
    let checkbox = boxes[0].rect();
    let trash = egui::pos2((rename.right() + checkbox.left()) / 2.0, row_y);
    harness.event(egui::Event::PointerMoved(trash));
    for pressed in [true, false] {
        harness.event(egui::Event::PointerButton {
            pos: trash,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::default(),
        });
    }
    harness.run();

    assert!(
        harness
            .state()
            .confirm_remove_worktree
            .lock()
            .unwrap()
            .is_some(),
        "the dirty row's trash button must open the removal dialog"
    );
    assert!(
        harness
            .query_by_label("Discard changes and remove")
            .is_some(),
        "a dirty worktree must name the force required, not offer a bare Remove"
    );
    assert!(
        !text_containing(&harness, "scratch.txt").is_empty(),
        "the dialog must enumerate the actual dirty file, not just say 'dirty'"
    );
}

fn text_containing(harness: &egui_kittest::Harness<'static, HiveApp>, needle: &str) -> Vec<String> {
    let mut found: Vec<String> = harness
        .query_all(egui_kittest::kittest::by())
        .flat_map(|node| [node.accesskit_node().label(), node.value()])
        .flatten()
        .filter(|text| text.contains(needle))
        .collect();
    found.sort();
    found.dedup();
    found
}

#[test]
fn bulk_remove_button_is_disabled_with_nothing_selected() {
    let (_tmp, repo, _clean_merged, _dirty, _unmerged) = setup_repo_with_three_worktrees();
    let app = app_with_worktrees(repo, vec![]);
    let mut harness = harness(app);
    harness.run();

    let button = harness.get_by_label("Remove 0 selected…");
    assert!(
        button.accesskit_node().is_disabled(),
        "the bulk-remove button should be disabled when nothing is selected"
    );
}

/// One repo, one primary, and `n` linked worktrees each on a fresh branch
/// checked out from `main`'s tip — i.e. clean and trivially merged (no
/// unique commits), matching `worktree_remove.rs`'s
/// `fresh_branch_at_main_is_landed_and_not_forced` fixture shape. Returns
/// `(TempDir, repo_path, worktree_paths)`.
fn setup_repo_with_n_clean_merged_worktrees(
    n: usize,
) -> (tempfile::TempDir, PathBuf, Vec<PathBuf>) {
    let tmp = tempfile::TempDir::new().unwrap();
    let repo = tmp.path().join("repo");
    fs::create_dir(&repo).unwrap();

    run(&repo, &["init", "-q", "-b", "main"]);
    run(&repo, &["config", "user.email", "test@example.com"]);
    run(&repo, &["config", "user.name", "Test"]);
    fs::write(repo.join("README.md"), "hello\n").unwrap();
    run(&repo, &["add", "."]);
    run(&repo, &["commit", "-qm", "init"]);

    let worktrees: Vec<PathBuf> = (0..n)
        .map(|i| {
            let path = tmp.path().join(format!("wt-clean-{i}"));
            run(
                &repo,
                &[
                    "worktree",
                    "add",
                    "-q",
                    "-b",
                    &format!("feat/clean-{i}"),
                    path.to_str().unwrap(),
                ],
            );
            path
        })
        .collect();

    (tmp, repo, worktrees)
}

fn branch_exists(repo: &Path, branch: &str) -> bool {
    git_cmd()
        .arg("-C")
        .arg(repo)
        .args([
            "rev-parse",
            "--verify",
            "--quiet",
            &format!("refs/heads/{branch}"),
        ])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// AC #3: selecting 5 merged+clean worktrees and confirming removal deletes
/// all 5 worktrees AND their branches, and `git worktree list` agrees
/// afterward. Drives `open_bulk_remove_worktree_confirm` (classification)
/// then `worktree_actions::run_bulk_removal` (the synchronous core
/// `execute_bulk_remove_worktrees`'s worker thread calls) directly against
/// real git state — no thread, no mocking of `remove_worktree`/`delete_branch`.
#[test]
fn run_bulk_removal_deletes_all_removable_worktrees_and_their_branches() {
    let (_tmp, repo, clean_worktrees) = setup_repo_with_n_clean_merged_worktrees(5);
    let mut worktree_refs = vec![wt("demo", repo.clone(), "main")];
    worktree_refs.extend(
        clean_worktrees
            .iter()
            .enumerate()
            .map(|(i, path)| wt("demo", path.clone(), &format!("feat/clean-{i}"))),
    );

    let mut app = app_with_worktrees(repo.clone(), worktree_refs);
    app.bulk_selected_worktrees = clean_worktrees.iter().cloned().collect();
    app.open_bulk_remove_worktree_confirm();

    let state = app
        .confirm_bulk_remove_worktrees
        .lock()
        .unwrap()
        .clone()
        .expect("all 5 clean+merged worktrees should open the dialog");
    assert_eq!(state.removable.len(), 5, "all 5 should be removable");
    assert!(
        state.needs_review.is_empty(),
        "none should need review: {:?}",
        state.needs_review
    );
    assert!(state.delete_branches, "delete_branches defaults on");

    let summary = switchbard_gui::worktree_actions::run_bulk_removal(
        &state.removable,
        state.delete_branches,
        || {},
    );

    assert_eq!(summary.removed.len(), 5, "all 5 should report removed");
    assert_eq!(
        summary.branch_deleted, 5,
        "all 5 branches should be deleted"
    );
    assert!(
        summary.first_error.is_none(),
        "got {:?}",
        summary.first_error
    );

    // `git worktree list` agrees: every clean worktree is gone from disk AND
    // from git's own bookkeeping.
    let output = git_cmd()
        .arg("-C")
        .arg(&repo)
        .args(["worktree", "list", "--porcelain"])
        .output()
        .unwrap();
    let listed = String::from_utf8_lossy(&output.stdout);
    for path in &clean_worktrees {
        assert!(
            !path.exists(),
            "worktree dir {path:?} should be gone from disk"
        );
        assert!(
            !listed.contains(path.to_str().unwrap()),
            "worktree {path:?} should be gone from `git worktree list`, got:\n{listed}"
        );
    }
    for i in 0..5 {
        assert!(
            !branch_exists(&repo, &format!("feat/clean-{i}")),
            "branch feat/clean-{i} should have been deleted"
        );
    }
}

/// Audit fix (PR #23 review): a `git branch -d` failure used to be silently
/// swallowed (`.is_ok()` and move on) — the worktree still got removed and
/// `branch_deleted` just quietly stayed low with no explanation. Constructs
/// a candidate whose worktree removal succeeds but whose `branch` name is
/// stale (already gone), so `delete_branch` fails; asserts the failure is
/// captured in `first_branch_error` and surfaces in `status_message`,
/// separately from `first_error` (which is reserved for a worktree-removal
/// failure — a strictly more severe, different class of problem).
#[test]
fn run_bulk_removal_reports_a_branch_delete_failure_without_swallowing_it() {
    use switchbard_core::BranchDeleteAssessment;
    use switchbard_gui::runtime::BulkRemoveCandidate;

    let (_tmp, repo, clean_worktrees) = setup_repo_with_n_clean_merged_worktrees(1);
    let worktree_path = clean_worktrees[0].clone();

    let candidate = BulkRemoveCandidate {
        repo_path: repo.clone(),
        worktree_path: worktree_path.clone(),
        display_name: "wt-clean-0".to_string(),
        // A branch name that was never actually created — `remove_worktree`
        // still succeeds (it only touches the worktree directory), but the
        // subsequent `git branch -d` on this name fails.
        branch: Some("feat/never-existed".to_string()),
        branch_assessment: Some(BranchDeleteAssessment {
            branch: "feat/never-existed".to_string(),
            other_checkouts: vec![],
            unmerged_commits: Some(0),
            compared_against: Some("main".to_string()),
        }),
        review_reason: None,
    };

    let summary = switchbard_gui::worktree_actions::run_bulk_removal(&[candidate], true, || {});

    assert_eq!(
        summary.removed,
        vec![worktree_path],
        "worktree removal itself should still succeed"
    );
    assert_eq!(
        summary.branch_deleted, 0,
        "the bad branch name should not count as deleted"
    );
    assert!(
        summary.first_error.is_none(),
        "worktree removal succeeded, so first_error stays empty"
    );
    let branch_err = summary
        .first_branch_error
        .clone()
        .expect("the branch-delete failure must not be silently swallowed");
    assert!(
        branch_err.contains("feat/never-existed"),
        "expected the branch name in the error, got: {branch_err}"
    );

    let msg = summary.status_message(1, 0);
    assert!(
        msg.contains("branch delete failed"),
        "status message should surface the branch-delete failure, got: {msg}"
    );
}
