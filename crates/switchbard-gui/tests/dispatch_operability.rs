//! TASK-43 — dispatch operability: the ambient chip, the tab badge, and the
//! Kill control, driven through real widgets in `egui_kittest`.
//!
//! The point of these is the *chrome*, not the pipeline. A user on the
//! Servers tab has no way to know an agent is running unless the top bar says
//! so, and no way to stop one unless a row offers a button — so every
//! assertion here is about what the always-visible surface renders and where
//! clicking it goes, with the app seeded into the state that surface is
//! supposed to describe.
//!
//! What is deliberately *not* asserted here: that confirming a kill actually
//! terminates a process. That crosses into a real signal against a real pgid,
//! which belongs (and lives) in `switchbard-core`'s
//! `killing_a_runs_process_group_via_its_sidecar_unblocks_the_pipelines_wait`
//! — the same bar `backlog_controls.rs`'s module doc sets for controls whose
//! only effect is a background thread.

mod common;

use egui_kittest::kittest::NodeT;
use std::path::PathBuf;

use common::{harness, seeded_app, REPO_PATH};
use egui_kittest::kittest::{self, Queryable};
use egui_kittest::Harness;
use switchbard_core::dispatch_inspect::{now_unix, DispatchRun, DispatchRunLiveness, SidecarDoubt};
use switchbard_core::{
    BacklogProject, BacklogTask, BacklogTaskSource, DispatchOptions, DISPATCHED_LABEL,
    DISPATCHING_LABEL, DISPATCH_FAILED_LABEL, DISPATCH_LABEL,
};
use switchbard_gui::app::HiveApp;
use switchbard_gui::runtime::ViewTab;

fn task(id: &str, labels: &[&str], notes: &str) -> BacklogTask {
    BacklogTask {
        id: id.to_string(),
        title: format!("{id} work"),
        status: "In Progress".to_string(),
        priority: "medium".to_string(),
        assignees: vec![],
        labels: labels.iter().map(|l| l.to_string()).collect(),
        dependencies: vec![],
        references: vec![],
        milestone: None,
        parent: None,
        created_date: None,
        updated_date: None,
        description: String::new(),
        implementation_plan: String::new(),
        implementation_notes: notes.to_string(),
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

/// A cached run that started `age_secs` ago, with no sidecar — the state of
/// every run the app cannot vouch for a process group for.
fn run(task_id: &str, age_secs: u64) -> DispatchRun {
    run_with(task_id, age_secs, DispatchRunLiveness::NoSidecar)
}

/// A cached run whose agent is verified alive and supervised by this process —
/// the *only* state in which the Kill button is allowed to render.
fn live_run(task_id: &str, age_secs: u64, pgid: i32) -> DispatchRun {
    run_with(
        task_id,
        age_secs,
        DispatchRunLiveness::Alive {
            pgid,
            supervised: true,
        },
    )
}

fn run_with(task_id: &str, age_secs: u64, liveness: DispatchRunLiveness) -> DispatchRun {
    DispatchRun {
        task_id: task_id.to_string(),
        branch: format!("dispatch/{}", task_id.to_lowercase()),
        worktree_path: PathBuf::from(format!("{REPO_PATH}/.worktrees/dispatch-{task_id}")),
        worktree_exists: false,
        log_path: Some(PathBuf::from("/tmp/switchbard-logs/dispatch.log")),
        prompt_path: None,
        started_at_unix: Some(now_unix().saturating_sub(age_secs)),
        log_bytes: 0,
        log_modified_unix: None,
        liveness,
    }
}

/// A headless app holding `tasks` in one project, with `runs` already cached
/// the way `workers::refresh_dispatch_runs` would have left them.
fn app_with(tasks: Vec<BacklogTask>, runs: Vec<DispatchRun>) -> HiveApp {
    let app = seeded_app();
    app.backlog_projects.lock().unwrap().insert(
        PathBuf::from(REPO_PATH),
        BacklogProject {
            root: PathBuf::from(REPO_PATH),
            cli_path: None,
            tasks,
            warnings: vec![],
            loaded_at_unix: 0,
            configured_statuses: vec![],
        },
    );
    let mut cached = app.dispatch_runs.lock().unwrap();
    for run in runs {
        cached.insert((PathBuf::from(REPO_PATH), run.task_id.clone()), run);
    }
    drop(cached);
    app
}

fn harness_with(tasks: Vec<BacklogTask>, runs: Vec<DispatchRun>) -> Harness<'static, HiveApp> {
    let mut harness = harness(app_with(tasks, runs));
    harness.run();
    harness
}

/// The chip's rendered text, if it rendered at all.
///
/// Matched by its glyph prefix rather than by an exact label, because the
/// running chip embeds a live elapsed time. `⚙ Settings` shares the gear, so
/// it is excluded by name — that collision is itself worth pinning: a future
/// chip whose copy drifted into "Settings" would silently break this probe.
fn dispatch_chip(harness: &Harness<'_, HiveApp>) -> Option<String> {
    harness
        .query_all(kittest::by())
        .filter_map(|node| node.accesskit_node().label())
        .find(|label| {
            (label.starts_with('⚙') || label.starts_with('⚠')) && !label.contains("Settings")
        })
}

/// Every piece of text in the tree containing `needle` — for assertions about
/// copy that embeds a live clock or a runtime-assigned pgid.
///
/// Reads a node's `value` as well as its `label`: a bare `ui.label` /
/// `ui.colored_label` surfaces its text as the node's *value* in egui's
/// accessibility tree, while interactive widgets (buttons, selectables)
/// surface theirs as the *label*. Checking only one of the two silently
/// misses half the UI — which cost real debugging time here.
///
/// Distinct strings only: egui hangs one `ui.label`'s text off more than one
/// node in the tree, so a raw collect reports the same copy two or three
/// times and turns any count-based assertion into a lie.
fn text_containing(harness: &Harness<'_, HiveApp>, needle: &str) -> Vec<String> {
    let mut found: Vec<String> = harness
        .query_all(kittest::by())
        .flat_map(|node| [node.accesskit_node().label(), node.value()])
        .flatten()
        .filter(|text| text.contains(needle))
        .collect();
    found.sort();
    found.dedup();
    found
}

// ─── The chip ───────────────────────────────────────────────────────────────

/// The default state of a Switchbard that has never dispatched anything: no
/// chip, no badge. Empty chrome is the failure mode this feature is most
/// likely to introduce, so it is asserted first.
#[test]
fn top_bar_shows_no_dispatch_chip_when_nothing_is_flagged() {
    let harness = harness_with(vec![task("TASK-1", &[], "")], vec![]);

    assert_eq!(dispatch_chip(&harness), None);
    assert!(
        harness.query_by_label("Dispatches").is_some(),
        "the tab itself still renders, unbadged"
    );
}

/// A task that already has its PR is finished work, not operational state —
/// it must not leave a chip lit forever.
#[test]
fn a_dispatched_task_awaiting_review_lights_no_chip() {
    let harness = harness_with(
        vec![task(
            "TASK-1",
            &[DISPATCHED_LABEL],
            "Dispatch PR: https://example.test/pr/1",
        )],
        vec![run("TASK-1", 3_000)],
    );

    assert_eq!(dispatch_chip(&harness), None);
    assert!(harness.query_by_label("Dispatches").is_some());
}

#[test]
fn top_bar_chip_counts_running_agents_and_navigates_to_the_dispatches_tab() {
    let mut harness = harness_with(
        vec![
            task("TASK-1", &[DISPATCHING_LABEL], ""),
            task("TASK-2", &[DISPATCHING_LABEL], ""),
        ],
        vec![run("TASK-1", 300), run("TASK-2", 60)],
    );

    let chip = dispatch_chip(&harness).expect("chip must render while agents are running");
    assert!(
        chip.starts_with("⚙ 2 running · 5m"),
        "chip should count both runs and lead with the oldest one's elapsed time: {chip:?}"
    );
    assert_eq!(
        harness.state().view_tab,
        ViewTab::Servers,
        "precondition: the chip is visible from a tab that is not Dispatches"
    );

    harness.get_by_label(chip.as_str()).click();
    harness.run();

    assert_eq!(harness.state().view_tab, ViewTab::Dispatch);
}

/// The attention flip, through the real widget: a failed run changes the
/// chip's *words*, not just its color (a color-only change is invisible to
/// this harness and, for a colorblind user, to the product).
#[test]
fn top_bar_chip_flips_to_attention_wording_when_a_run_failed() {
    let harness = harness_with(
        vec![
            task(
                "TASK-1",
                &[DISPATCH_FAILED_LABEL],
                "Dispatch failed: claude exited with Some(1)",
            ),
            task("TASK-2", &[DISPATCHING_LABEL], ""),
        ],
        vec![run("TASK-2", 120)],
    );

    assert_eq!(
        dispatch_chip(&harness).as_deref(),
        Some("⚠ 1 dispatch run needs attention · 1 running")
    );
}

/// A run past its own advisory staleness threshold is reported as needing
/// attention rather than as healthily running — the stalled case, end to
/// end. TASK-46: this is a *report*, not a kill — the run underneath is
/// still going, and this test only asserts what the chip says about it.
#[test]
fn top_bar_chip_treats_a_run_past_its_stale_after_threshold_as_needing_attention() {
    let past_threshold = DispatchOptions::default().stale_after.as_secs() + 60;
    let harness = harness_with(
        vec![task("TASK-1", &[DISPATCHING_LABEL], "")],
        vec![run("TASK-1", past_threshold)],
    );

    assert_eq!(
        dispatch_chip(&harness).as_deref(),
        Some("⚠ 1 dispatch run needs attention")
    );
}

// ─── The tab badge ──────────────────────────────────────────────────────────

/// The badge counts *runs* — in flight plus needing attention — so a long
/// queue of flagged-but-unstarted tasks does not inflate it.
#[test]
fn dispatches_tab_badge_counts_runs_not_queue_depth() {
    let harness = harness_with(
        vec![
            task("TASK-1", &[DISPATCHING_LABEL], ""),
            task("TASK-2", &[DISPATCH_FAILED_LABEL], ""),
            task("TASK-3", &[DISPATCH_LABEL], ""),
            task("TASK-4", &[DISPATCH_LABEL], ""),
        ],
        vec![run("TASK-1", 90)],
    );

    assert!(
        harness.query_by_label("Dispatches (2)").is_some(),
        "one in flight + one failed; the two queued tasks are not runs"
    );
    assert!(
        harness.query_by_label("Dispatches").is_none(),
        "the badged label replaces the plain one rather than doubling it"
    );
    // The queue still reaches the user through the chip, just not the badge.
    let chip = dispatch_chip(&harness).expect("chip");
    assert!(chip.contains("needs attention"), "{chip:?}");
}

// ─── The Kill control ───────────────────────────────────────────────────────

/// TASK-46: no code path promises a hard-kill deadline any more — an
/// in-flight row just says how long it has been running, with no countdown
/// to a kill that no longer happens.
#[test]
fn an_in_flight_row_shows_its_running_time_and_no_hard_kill_deadline() {
    let mut app = app_with(
        vec![task("TASK-1", &[DISPATCHING_LABEL], "")],
        vec![live_run("TASK-1", 300, 4242)],
    );
    app.view_tab = ViewTab::Dispatch;
    let mut harness = harness(app);
    harness.run();

    assert!(
        text_containing(&harness, "hard kill in").is_empty(),
        "TASK-46 removed the wall-clock kill; nothing should promise one"
    );
    let running = text_containing(&harness, "running 5m");
    assert_eq!(running.len(), 1, "{running:?}");
}

/// A run past its advisory staleness threshold still reads as *running* —
/// just flagged for a human to go look — never as something about to be
/// killed.
#[test]
fn a_stale_in_flight_row_still_reads_as_running_not_as_doomed() {
    let mut app = app_with(
        vec![task("TASK-1", &[DISPATCHING_LABEL], "")],
        vec![live_run("TASK-1", 31 * 60, 4242)],
    );
    app.view_tab = ViewTab::Dispatch;
    let mut harness = harness(app);
    harness.run();

    assert!(
        text_containing(&harness, "hard kill in").is_empty(),
        "a stale run must not promise an automatic kill"
    );
    let check_on_it = text_containing(&harness, "check on it");
    assert_eq!(check_on_it.len(), 1, "{check_on_it:?}");
    assert!(
        check_on_it[0].starts_with("running "),
        "the stale copy must still lead with \"running\", not with a verdict about being killed: {:?}",
        check_on_it[0]
    );
}

/// Arm, read the confirmation, back out. The confirm step is the whole point
/// of the control — a bare Kill button next to a run someone spent 20 minutes
/// waiting on is a misclick away from a disaster.
#[test]
fn the_kill_button_is_confirm_armed_and_cancellable() {
    let mut app = app_with(
        vec![task("TASK-1", &[DISPATCHING_LABEL], "")],
        vec![live_run("TASK-1", 300, 4242)],
    );
    app.view_tab = ViewTab::Dispatch;
    let mut harness = harness(app);
    harness.run();

    assert!(harness.state().dispatch_kill_confirm.is_none());
    harness.get_by_label("Kill run").click();
    harness.run();

    assert_eq!(
        harness.state().dispatch_kill_confirm,
        Some((PathBuf::from(REPO_PATH), "TASK-1".to_string())),
        "arming is keyed by (repo root, task id) — a task id alone is not unique"
    );
    assert!(
        !text_containing(&harness, "pgid 4242").is_empty(),
        "the confirmation names the process group it is about to signal"
    );
    assert!(harness.query_by_label("Confirm kill").is_some());

    harness.get_by_label("Cancel kill").click();
    harness.run();

    assert!(harness.state().dispatch_kill_confirm.is_none());
    assert!(harness.query_by_label("Confirm kill").is_none());
    assert!(harness.query_by_label("Kill run").is_some());
}

/// No verified process, no button — and (TASK-46 made this universally true,
/// but it is still worth pinning here) no deadline copy either.
///
/// This is the state of every run started by a Switchbard that has since
/// restarted. The app has no evidence any agent is out there, so it must not
/// promise a hard kill that nothing performs any more (audit F3, and now
/// true for every run, not just an unsupervised one) as well as withholding
/// the button.
#[test]
fn a_run_without_a_verified_process_offers_no_kill_and_no_deadline() {
    let mut app = app_with(
        vec![task("TASK-1", &[DISPATCHING_LABEL], "")],
        vec![run("TASK-1", 300)],
    );
    app.view_tab = ViewTab::Dispatch;
    let mut harness = harness(app);
    harness.run();

    assert!(harness.query_by_label("Kill run").is_none());
    assert!(
        text_containing(&harness, "hard kill in").is_empty(),
        "no code path promises a hard-kill deadline any more"
    );
    assert!(
        !text_containing(&harness, "unsupervised").is_empty(),
        "the row must say why, not just render less"
    );
    assert!(
        !text_containing(&harness, "running 5m").is_empty(),
        "the row still renders — only the deadline and the kill are withheld"
    );
}

/// F1, the blocking finding, at the UI boundary: a sidecar left behind by a
/// force-quit Switchbard names a pgid the OS may since have reissued. However
/// alive that number looks, an unauthenticated sidecar must never produce a
/// button — the whole danger of the original design was that the confirm
/// dialog reassured the user in exactly this case.
///
/// Each doubt is pinned separately because each comes from a different guard
/// (format version, boot epoch, the probe itself) and any one of them
/// regressing independently would re-arm the weapon.
#[test]
fn an_unverifiable_sidecar_never_arms_a_kill_whatever_the_reason() {
    for doubt in [
        SidecarDoubt::LegacyFormat,
        SidecarDoubt::StaleBoot,
        SidecarDoubt::ProbeFailed,
    ] {
        let mut app = app_with(
            vec![task("TASK-1", &[DISPATCHING_LABEL], "")],
            vec![run_with(
                "TASK-1",
                300,
                DispatchRunLiveness::Unverifiable(doubt),
            )],
        );
        app.view_tab = ViewTab::Dispatch;
        let mut harness = harness(app);
        harness.run();

        assert!(
            harness.query_by_label("Kill run").is_none(),
            "{doubt:?} must not arm a kill"
        );
        assert!(
            text_containing(&harness, "hard kill in").is_empty(),
            "{doubt:?} must not promise a deadline"
        );
        assert!(
            !text_containing(&harness, "unverified").is_empty(),
            "{doubt:?} must explain itself"
        );
    }
}

/// F1's other half: a run whose group is *positively verified gone* while its
/// task is still claimed. Before the liveness probe this row sat under "In
/// flight" forever with a live Kill button, because an empty log is
/// indistinguishable from a healthy run. It must now read as abandoned, count
/// toward the attention chip, and offer no kill.
#[test]
fn a_run_whose_group_died_reads_as_abandoned_and_offers_no_kill() {
    let mut app = app_with(
        vec![task("TASK-1", &[DISPATCHING_LABEL], "")],
        vec![run_with("TASK-1", 300, DispatchRunLiveness::Gone)],
    );
    app.view_tab = ViewTab::Dispatch;
    let mut harness = harness(app);
    harness.run();

    assert!(harness.query_by_label("Kill run").is_none());
    assert!(
        !text_containing(&harness, "Claimed, but nothing is running").is_empty(),
        "the row belongs in the attention section, not under 'In flight'"
    );
    assert!(
        text_containing(&harness, "hard kill in").is_empty(),
        "nothing is running, so nothing is counting down"
    );
    assert_eq!(
        dispatch_chip(&harness).as_deref(),
        Some("⚠ 1 dispatch run needs attention"),
        "and the chip must say so from every other tab"
    );
}

/// The unsupervised-but-alive case (audit F3): the agent is verifiably still
/// running, but the Switchbard that spawned it is gone. Killing is allowed —
/// the process was positively identified — but the copy must not promise the
/// release bookkeeping that only a live pipeline can do (and, since TASK-46,
/// must not promise a deadline either — there isn't one for any run).
#[test]
fn an_unsupervised_live_run_offers_an_honestly_labelled_kill() {
    let mut app = app_with(
        vec![task("TASK-1", &[DISPATCHING_LABEL], "")],
        vec![run_with(
            "TASK-1",
            300,
            DispatchRunLiveness::Alive {
                pgid: 4242,
                supervised: false,
            },
        )],
    );
    app.view_tab = ViewTab::Dispatch;
    let mut harness = harness(app);
    harness.run();

    assert!(
        text_containing(&harness, "hard kill in").is_empty(),
        "no code path promises a hard-kill deadline any more"
    );
    assert!(
        !text_containing(&harness, "nothing will release the task when it ends").is_empty(),
        "the row has to say the release step won't happen on its own"
    );

    harness.get_by_label("Kill run").click();
    harness.run();

    let confirm = text_containing(&harness, "pgid 4242");
    assert_eq!(confirm.len(), 1, "{confirm:?}");
    assert!(
        confirm[0].contains("stays on `dispatching`"),
        "the confirmation must not promise a release nothing will perform: {:?}",
        confirm[0]
    );
}

/// A supervised kill *does* get to promise the release, because the pipeline
/// really is blocked on that process group. The two messages are the point of
/// the distinction, so pin both sides.
#[test]
fn a_supervised_kill_promises_the_release_the_pipeline_will_actually_do() {
    let mut app = app_with(
        vec![task("TASK-1", &[DISPATCHING_LABEL], "")],
        vec![live_run("TASK-1", 300, 4242)],
    );
    app.view_tab = ViewTab::Dispatch;
    let mut harness = harness(app);
    harness.run();

    harness.get_by_label("Kill run").click();
    harness.run();

    let confirm = text_containing(&harness, "pgid 4242");
    assert_eq!(confirm.len(), 1, "{confirm:?}");
    assert!(
        confirm[0].contains("released as dispatch-failed"),
        "{:?}",
        confirm[0]
    );
    assert!(!confirm[0].contains("stays on `dispatching`"));
}

/// A queued task has no process to kill, even though the Dispatch view lists/// A queued task has no process to kill, even though the Dispatch view lists
/// it a section away from the in-flight rows.
#[test]
fn a_queued_task_offers_no_kill_button() {
    let mut app = app_with(vec![task("TASK-1", &[DISPATCH_LABEL], "")], vec![]);
    app.view_tab = ViewTab::Dispatch;
    let mut harness = harness(app);
    harness.run();

    assert!(harness.query_by_label("Kill run").is_none());
}
