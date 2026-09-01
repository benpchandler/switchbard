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
/// express "the user clicked *there*" — and "there" is exactly what these
/// tests are about: the blank parts of a row that no widget claims.
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

/// The widest piece of the linked row's header that no widget occupies.
///
/// Derived from the row's actual widget rects rather than from a named
/// neighbour, so it survives the row's contents changing — it was previously
/// anchored to the head SHA, which has since been deleted from the row.
fn blank_spot_in_row(harness: &egui_kittest::Harness<'static, HiveApp>) -> egui::Pos2 {
    let name = harness.get_by_label("wt-clean-merged").rect();
    let mut spans = Vec::new();
    nodes_on_row_line(harness, name.center().y, &mut spans);
    spans.sort_by(|a: &egui::Rect, b| a.left().total_cmp(&b.left()));

    let mut best: Option<(f32, egui::Pos2)> = None;
    for pair in spans.windows(2) {
        let gap = pair[1].left() - pair[0].right();
        if best.is_none_or(|(w, _)| gap > w) {
            best = Some((
                gap,
                egui::pos2((pair[0].right() + pair[1].left()) / 2.0, name.center().y),
            ));
        }
    }
    let (width, pos) = best.expect("the row must have at least two widgets");
    assert!(
        width > 8.0,
        "expected a real gap of blank row space, found only {width}px"
    );
    assert!(
        !spans.iter().any(|r| r.contains(pos)),
        "the probe point must be blank row space, not a widget: {pos:?}"
    );
    pos
}

/// Every accesskit node whose vertical span crosses `row_y`, in row order.
fn nodes_on_row_line(
    harness: &egui_kittest::Harness<'static, HiveApp>,
    row_y: f32,
    out: &mut Vec<egui::Rect>,
) {
    fn walk(node: &egui_kittest::Node<'_>, row_y: f32, out: &mut Vec<egui::Rect>) {
        if let Some(b) = node.accesskit_node().raw_bounds() {
            if (b.y0 as f32) <= row_y && row_y <= (b.y1 as f32) {
                out.push(egui::Rect::from_min_max(
                    egui::pos2(b.x0 as f32, b.y0 as f32),
                    egui::pos2(b.x1 as f32, b.y1 as f32),
                ));
            }
        }
        for child in node.children() {
            walk(&child, row_y, out);
        }
    }
    walk(&harness.root(), row_y, out);
}

/// Centre of the row's expand/collapse triangle.
///
/// `CollapsingState` draws it with a bare `ui.interact`, so it carries no
/// accesskit label to query by — but it is the only widget on the row's line
/// left of the name, and it is exactly one `spacing.indent` wide. Locating it
/// that way (rather than by a pixel offset) keeps the test honest if the
/// row's left cluster ever changes.
fn expand_triangle_center(
    harness: &egui_kittest::Harness<'static, HiveApp>,
    name: egui::Rect,
) -> egui::Pos2 {
    let mut spans = Vec::new();
    nodes_on_row_line(harness, name.center().y, &mut spans);
    let toggle = spans
        .into_iter()
        .filter(|r| r.right() <= name.left())
        .max_by(|a, b| a.right().total_cmp(&b.right()))
        .expect("the row must have a widget left of its name");
    assert_eq!(
        toggle.width(),
        egui::Style::default().spacing.indent,
        "expected the collapsing toggle (one indent wide), got {toggle:?}"
    );
    toggle.center()
}

/// The ask: clicking *anywhere* in the row selects it, not just the name.
/// This clicks the dead space between the two clusters — the largest target
/// in the row and, before this, the only part of it that did nothing.
#[test]
fn clicking_blank_row_space_selects_the_worktree() {
    let (_tmp, repo, clean_merged, _dirty, _unmerged) = setup_repo_with_three_worktrees();
    let worktrees = vec![
        wt("demo", repo.clone(), "main"),
        wt("demo", clean_merged.clone(), "feat/clean-merged"),
    ];
    let mut harness = harness(app_with_worktrees(repo.clone(), worktrees));
    harness.run();
    assert!(
        harness.state().bulk_selected_worktrees.is_empty(),
        "precondition: nothing selected"
    );

    let pos = blank_spot_in_row(&harness);
    click_at(&harness, pos);
    harness.run();
    assert!(
        harness
            .state()
            .bulk_selected_worktrees
            .contains(&clean_merged),
        "clicking blank row space should select the row"
    );

    // Toggle, not a one-way latch.
    click_at(&harness, pos);
    harness.run();
    assert!(
        harness.state().bulk_selected_worktrees.is_empty(),
        "clicking blank row space again should deselect it"
    );
}

/// A hover-only label inside the row (here the head SHA, which carries a
/// tooltip) must select the row rather than swallow the click. This is the
/// case the previous implementation got wrong: it assumed a non-clickable
/// widget sitting on top would absorb the gesture, so it put the gesture on
/// the name alone. egui's hit test filters click candidates by
/// `Sense::senses_click`, so a hover-only label is never a click target and
/// cannot absorb anything.
#[test]
fn clicking_a_hover_only_label_in_the_row_selects_the_worktree() {
    let (_tmp, repo, clean_merged, _dirty, _unmerged) = setup_repo_with_three_worktrees();
    let worktrees = vec![
        wt("demo", repo.clone(), "main"),
        wt("demo", clean_merged.clone(), "feat/clean-merged"),
    ];
    let mut harness = harness(app_with_worktrees(repo.clone(), worktrees));
    harness.run();

    // A hover-only label with a tooltip, one per row. (This was the head
    // SHA until the row shed it; the trunk chip's pending placeholder is the
    // same kind of widget and outlives layout changes better.)
    let chip: Vec<_> = harness.get_all_by_label("trunk ...").collect();
    assert_eq!(chip.len(), 2, "primary and linked rows each render one");
    chip[1].click();
    harness.run();
    assert!(
        harness
            .state()
            .bulk_selected_worktrees
            .contains(&clean_merged),
        "clicking a hover-only label in the row should select the row"
    );
}

/// The row's name keeps working as a select target — the behaviour this
/// change widens rather than replaces.
#[test]
fn clicking_the_row_name_still_selects_the_worktree() {
    let (_tmp, repo, clean_merged, _dirty, _unmerged) = setup_repo_with_three_worktrees();
    let worktrees = vec![
        wt("demo", repo.clone(), "main"),
        wt("demo", clean_merged.clone(), "feat/clean-merged"),
    ];
    let mut harness = harness(app_with_worktrees(repo.clone(), worktrees));
    harness.run();

    harness.get_by_label("wt-clean-merged").click();
    harness.run();
    assert!(
        harness
            .state()
            .bulk_selected_worktrees
            .contains(&clean_merged),
        "clicking the row name should still select it"
    );
}

/// The half that keeps the row honest. A full-row click target is only safe
/// because egui resolves an overlapping click to the *last-registered*
/// click-sensing widget, and the row's sensed `Ui` registers before its
/// children (`Ui::remember_min_rect` re-registers with `move_to_top: false`,
/// so correcting the rect at the end does not reorder it). That is a property
/// of egui's interaction ordering, not of our code, so it gets pinned: a
/// refactor that registers the row rect after the children would silently
/// start swallowing every button in it.
#[test]
fn row_click_selects_but_buttons_still_win() {
    let (_tmp, repo, clean_merged, _dirty, _unmerged) = setup_repo_with_three_worktrees();
    let worktrees = vec![
        wt("demo", repo.clone(), "main"),
        wt("demo", clean_merged.clone(), "feat/clean-merged"),
    ];
    let mut harness = harness(app_with_worktrees(repo.clone(), worktrees));
    harness.run();

    // Two rows render a Rename button (primary + linked); take the linked
    // row's, which is the one sitting inside the row-select target.
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
        "clicking Rename must not also select the row"
    );
}

/// The row's own checkbox must toggle selection exactly once. A full-row
/// click target that also fired would cancel it out and leave the checkbox
/// looking broken.
#[test]
fn the_row_checkbox_toggles_selection_exactly_once() {
    let (_tmp, repo, clean_merged, _dirty, _unmerged) = setup_repo_with_three_worktrees();
    let worktrees = vec![
        wt("demo", repo.clone(), "main"),
        wt("demo", clean_merged.clone(), "feat/clean-merged"),
    ];
    let mut harness = harness(app_with_worktrees(repo.clone(), worktrees));
    harness.run();

    // The view has other checkboxes (top-bar toggles), so pick the one sitting
    // on the linked row's line — the primary row renders none, being no
    // candidate for removal.
    let row_y = harness.get_by_label("wt-clean-merged").rect().center().y;
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

/// The expand/collapse triangle is `show_header`'s own widget, laid out to
/// the left of the sensed row `Ui` and outside its rect. It must keep
/// toggling the row open without also selecting it — otherwise widening the
/// click target would have taken expansion away.
#[test]
fn the_expand_triangle_still_toggles_without_selecting() {
    let (_tmp, repo, clean_merged, _dirty, _unmerged) = setup_repo_with_three_worktrees();
    let worktrees = vec![
        wt("demo", repo.clone(), "main"),
        wt("demo", clean_merged.clone(), "feat/clean-merged"),
    ];
    let mut harness = harness(app_with_worktrees(repo.clone(), worktrees));
    harness.run();

    let name = harness.get_by_label("wt-clean-merged").rect();
    assert!(
        harness.query_by_label("nothing detected here").is_none(),
        "precondition: the row starts collapsed"
    );

    click_at(&harness, expand_triangle_center(&harness, name));
    harness.run();

    assert!(
        harness.query_by_label("nothing detected here").is_some(),
        "the triangle must still expand the row"
    );
    assert!(
        harness.state().bulk_selected_worktrees.is_empty(),
        "expanding a row must not also select it"
    );
}

/// The gesture is scoped to the header, not the whole frame. An expanded
/// row's body holds its own actions (start/stop a service, open a port), and
/// clicking around in there must not silently arm a removal.
#[test]
fn clicking_an_expanded_rows_body_does_not_select_it() {
    let (_tmp, repo, clean_merged, _dirty, _unmerged) = setup_repo_with_three_worktrees();
    let worktrees = vec![
        wt("demo", repo.clone(), "main"),
        wt("demo", clean_merged.clone(), "feat/clean-merged"),
    ];
    let mut harness = harness(app_with_worktrees(repo.clone(), worktrees));
    harness.run();

    let name = harness.get_by_label("wt-clean-merged").rect();
    click_at(&harness, expand_triangle_center(&harness, name));
    harness.run();
    assert!(
        harness.state().bulk_selected_worktrees.is_empty(),
        "precondition: expanding did not select"
    );

    harness.get_by_label("nothing detected here").click();
    harness.run();
    assert!(
        harness.state().bulk_selected_worktrees.is_empty(),
        "clicking the row body must not select the row"
    );
}

/// The trash button opens the single-row removal dialog. It sits inside the
/// sensed row, so it is the other half of "buttons still win".
#[test]
fn the_trash_button_still_wins_its_own_click() {
    let (_tmp, repo, clean_merged, _dirty, _unmerged) = setup_repo_with_three_worktrees();
    let worktrees = vec![
        wt("demo", repo.clone(), "main"),
        wt("demo", clean_merged.clone(), "feat/clean-merged"),
    ];
    let mut harness = harness(app_with_worktrees(repo.clone(), worktrees));
    harness.run();

    // Right-to-left trailing cluster: SHA, Rename, trash, checkbox. The trash
    // button is the icon between Rename and the checkbox on the linked row.
    let rename = harness.get_all_by_label("Rename").nth(1).unwrap().rect();
    let row_y = rename.center().y;
    let boxes: Vec<_> = harness
        .get_all_by_role(egui::accesskit::Role::CheckBox)
        .filter(|n| n.rect().y_range().contains(row_y))
        .collect();
    let checkbox = boxes[0].rect();
    let trash = egui::pos2((rename.left() + checkbox.right()) / 2.0, row_y);

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
        "clicking the trash button must not also select the row"
    );
}

/// The primary worktree renders no checkbox and is dropped from the candidate
/// list, so a row-click that selected it would offer a selection that
/// evaporates.
#[test]
fn clicking_the_primary_row_selects_nothing() {
    let (_tmp, repo, clean_merged, _dirty, _unmerged) = setup_repo_with_three_worktrees();
    let worktrees = vec![
        wt("demo", repo.clone(), "main"),
        wt("demo", clean_merged.clone(), "feat/clean-merged"),
    ];
    let mut harness = harness(app_with_worktrees(repo.clone(), worktrees));
    harness.run();

    let chip: Vec<_> = harness.get_all_by_label("trunk ...").collect();
    chip[0].click(); // the primary row's
    harness.run();
    assert!(
        harness.state().bulk_selected_worktrees.is_empty(),
        "the primary row must not be selectable by clicking it"
    );
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
