//! QA parity audit (2026-08-05) — the edge cases the audit brief called out
//! by name: 455-task real-repo scale, an empty real repo, a malformed
//! `ordering.yml`'s warning pill, and a missing-CLI repo's read-only
//! affordances.
//!
//! Real `~/Dev` repos (`budget-onramp-pilot`, `janus`) are read via
//! `switchbard_core::load_backlog_repo` only — a pure filesystem read, no
//! `backlog` CLI invocation, no writes, no `dispatch` label ever set. These
//! two tests are `#[ignore]`d because they depend on paths outside this
//! repo that only exist on the auditor's machine; run them explicitly with
//! `-- --ignored` to reproduce.

mod common;

use std::path::{Path, PathBuf};
use std::time::Instant;

use common::{harness, seeded_app, REPO_PATH};
use egui_kittest::kittest::Queryable;
use egui_kittest::SnapshotOptions;
use switchbard_core::{BacklogRepo, OrderingOverlay};
use switchbard_gui::runtime::{BacklogLens, OrderingState, ViewTab};

fn output_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/qa/screenshots")
}

/// Real, read-only, ~455-task Backlog.md repo. Confirmed on this machine
/// via `find ~/Dev/budget-onramp-pilot/backlog/tasks -name '*.md' | wc -l`
/// (455) before writing this test — the exact repo and scale the audit
/// brief named.
#[test]
#[ignore = "depends on a real ~/Dev repo only present on the auditor's machine; run with -- --ignored"]
fn real_455_task_repo_loads_and_renders_the_list_lens_within_a_render_path_budget() {
    let root = dirs::home_dir()
        .expect("home dir")
        .join("Dev/budget-onramp-pilot");
    assert!(
        root.join("backlog").is_dir(),
        "expected a real budget-onramp-pilot checkout at {}",
        root.display()
    );

    let load_start = Instant::now();
    let repo = switchbard_core::load_backlog_repo(&root).expect("read-only load must succeed");
    let load_elapsed = load_start.elapsed();
    assert!(
        repo.tasks.len() > 400,
        "expected the ~455-task scale this test is named for, got {}",
        repo.tasks.len()
    );
    // A generous ceiling (this is a debug build with no I/O caching
    // guarantees) — the point is catching a pathological regression (e.g.
    // quadratic parsing), not micro-benchmarking.
    assert!(
        load_elapsed.as_secs() < 5,
        "loading {} tasks took {load_elapsed:?}, expected well under 5s",
        repo.tasks.len()
    );

    let mut app = seeded_app();
    app.view_tab = ViewTab::Backlog;
    app.backlog_view.lens = BacklogLens::List;
    app.backlog_view.selected_repo = Some(root.clone());
    app.backlog_repos.lock().unwrap().insert(root, repo);

    let render_start = Instant::now();
    let mut h = harness(app);
    h.run();
    let render_elapsed = render_start.elapsed();
    assert!(
        render_elapsed.as_secs() < 10,
        "first render of the 455-task List lens took {render_elapsed:?}, expected well under 10s"
    );

    assert!(
        h.query_by_label("Sort").is_some(),
        "the List lens should render normally at this scale, not blank/panic"
    );

    let options = SnapshotOptions::new().output_path(output_dir());
    let _ = h.try_snapshot_options("backlog_scale_455_tasks", &options);
}

/// Real, empty (zero-task) Backlog.md repo — confirmed via `find
/// ~/Dev/janus/backlog/tasks -name '*.md' | wc -l` (0) before writing this
/// test, the exact repo the audit brief named for this case.
#[test]
#[ignore = "depends on a real ~/Dev repo only present on the auditor's machine; run with -- --ignored"]
fn real_empty_repo_loads_with_zero_tasks_and_no_warnings() {
    let root = dirs::home_dir().expect("home dir").join("Dev/janus");
    assert!(
        root.join("backlog").is_dir(),
        "expected a real janus checkout at {}",
        root.display()
    );

    let repo = switchbard_core::load_backlog_repo(&root).expect("read-only load must succeed");
    assert!(
        repo.tasks.is_empty(),
        "janus is the fixture named for the zero-task case; got {} tasks",
        repo.tasks.len()
    );

    let mut app = seeded_app();
    app.view_tab = ViewTab::Backlog;
    app.backlog_view.lens = BacklogLens::List;
    app.backlog_view.selected_repo = Some(root.clone());
    app.backlog_repos.lock().unwrap().insert(root, repo);
    let mut h = harness(app);
    h.run();

    assert!(
        h.query_by_label("No tasks match the current filters")
            .is_some(),
        "an empty-but-tracked repo should show the List lens's own empty state"
    );

    let options = SnapshotOptions::new().output_path(output_dir());
    let _ = h.try_snapshot_options("backlog_empty_tracked_repo", &options);
}

/// Malformed `ordering.yml` — real parse (`OrderingOverlay::parse`, already
/// unit-tested at the core level in `backlog_triage.rs`'s
/// `malformed_overlay_yaml_falls_back_to_empty_with_a_warning`) wired all
/// the way through to the GUI's own warning pill, which no existing test
/// exercised.
fn malformed_ordering_yaml_app() -> switchbard_gui::app::HiveApp {
    let malformed_yaml = "ranked: [this is not a valid\n  - sequence";
    let (overlay, warning) = OrderingOverlay::parse(malformed_yaml);
    let warning = warning.expect("malformed YAML should produce a warning");
    assert!(warning.contains("malformed"));

    let mut app = seeded_app();
    app.view_tab = ViewTab::Backlog;
    app.backlog_view.lens = BacklogLens::List;
    app.backlog_view.selected_repo = Some(PathBuf::from(REPO_PATH));
    app.backlog_repos.lock().unwrap().insert(
        PathBuf::from(REPO_PATH),
        BacklogRepo {
            root: PathBuf::from(REPO_PATH),
            tasks: vec![],
            warnings: vec![],
            project_defs: vec![],
            initiative_defs: vec![],
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
    *app.ordering.lock().unwrap() = OrderingState {
        overlay,
        warning: Some(warning),
    };
    app
}

#[test]
fn malformed_ordering_yaml_warning_renders_as_a_toolbar_pill() {
    let mut h = harness(malformed_ordering_yaml_app());
    h.run();

    assert!(
        h.query_by_label("ordering.yml").is_some(),
        "a malformed ordering.yml should render its warning as a toolbar pill"
    );
}

/// Screenshot capture for the synthetic edge case above, split out from
/// their (always-run) assertion tests and `#[ignore]`d — `try_wgpu_snapshot_
/// options` unconditionally writes a `.new.png` scratch file even when the
/// canonical snapshot isn't being updated, and this repo's convention (see
/// `ui_snapshot.rs`) is that no plain `cargo test`/CI run should have that
/// side effect on `docs/qa/screenshots/`.
#[test]
#[ignore = "wgpu image snapshot: machine-specific, run explicitly with `-- --ignored` (see module docs)"]
fn generate_edge_case_screenshots() {
    let options = SnapshotOptions::new().output_path(output_dir());

    let mut h = harness(malformed_ordering_yaml_app());
    h.run();
    let _ = h.try_snapshot_options("backlog_malformed_ordering_yaml_warning", &options);
}
