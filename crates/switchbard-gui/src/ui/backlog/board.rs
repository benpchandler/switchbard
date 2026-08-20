//! The Board lens: per-status kanban columns, cross-repo, with drag-to-change
//! status writing through the `backlog` CLI (task-15 AC #1).
//!
//! Columns come from `switchbard_core::ordered_status_vocabulary` (owner
//! UX pass, 2026-08-05: "all projects should share a common set of statuses
//! across every view") — the same status list every other status-listing
//! surface (List's filter, the detail-pane editor, Statistics) now consumes
//! too, so Board can no longer show a column List's filter doesn't offer or
//! vice versa.
//!
//! Drag-and-drop uses egui's native `dnd_drop_zone` on the receiving side
//! and a hand-rolled `Sense::click_and_drag()` source per card (TASK-29 —
//! see `render_strip`'s doc for why `Ui::dnd_drag_source` couldn't be used
//! once cards needed a nested, independently-clickable checkbox). Only
//! editable tasks (active source, CLI available) are drag sources —
//! draft/completed/archived cards render as plain, non-draggable strips.
//!
//! ## Optimistic move + drop feedback (task-42)
//!
//! A drop's status change writes through the real `backlog` CLI
//! (`HiveApp::spawn_board_move_save`), which is a 0.5-1.5s subprocess round
//! trip — rendering strictly off `HiveApp::backlog_projects` would leave a
//! dropped card sitting in its origin column, motionless, for that entire
//! window. `apply_drop` instead writes a `PendingBoardMove` into
//! `app.backlog_view.pending_moves` (see that type's doc, runtime/mod.rs)
//! synchronously, same frame as the drop; `render_column`'s column
//! membership check (`card_shows_in_column`) reads that overlay ahead of the
//! task's real `status`, so the card jumps to its destination column
//! immediately, painted with a dimmed/"saving" treatment
//! (`CardMotion::Saving`, in `paint_card`). `resolve_pending_moves`, called
//! once per frame before any column is built, is the only place an entry
//! ever leaves `pending_moves` — see its doc for the resolution signal and
//! the timeout fallback.
//!
//! **Post-review revision (independent audit of the first version):** a
//! pending move now resolves only off its *own* drop's own save completing
//! (a per-drop `generation` token, carried by `HiveApp::spawn_board_move_save`
//! into `HiveApp::board_move_outcomes`) — never off an unrelated cache
//! reload, which the first version used and which could resolve (and
//! visually snap back) a still-in-flight move early. `apply_drop` also now
//! compares a drop's target against the card's *effective* (overlay-aware)
//! status rather than its real one, so dragging a card back out of its own
//! pending destination is recognized as a genuine new move instead of a
//! silent no-op, and `HiveApp::task_write_locks` serializes every writer's
//! saves per task (not just Board drops against each other, as of a second
//! post-review pass — see that field's doc) so two racing writes to the
//! same task can't leave on-disk state that doesn't match the user's last
//! gesture. See `resolve_pending_moves` and `apply_drop` for the detail.

use super::{dispatch_ui, format, list, scoped_projects, selection, Pending, Snapshot, TaskRow};
use crate::app::HiveApp;
use crate::runtime::{BacklogTaskKey, PendingBoardMove};
use crate::ui::theme;
use eframe::egui;
use std::time::{Duration, Instant};
use switchbard_core::{
    humanize_age, ordered_status_vocabulary, parse_backlog_datetime_unix, BacklogTask,
    BacklogTaskPatch,
};

/// How long a `PendingBoardMove` is allowed to sit without its own
/// generation's outcome ever being reported before it gives up and clears
/// itself (see that type's doc, runtime/mod.rs, for why this is now purely
/// a last-resort backstop rather than the primary resolution signal).
/// Comfortably above the 0.5-1.5s CLI round trip this task's own mission
/// brief names, so a normal save — success or failure — always resolves off
/// its own completion well before this fires; this only fires if that
/// report is somehow lost (e.g. the save thread panics).
///
/// Measured against `PendingBoardMove::queued_at`, which `resolve_pending_
/// moves` refreshes to "now" the moment the save actually *starts* (N9,
/// post-review revision — `HiveApp::board_move_started`), not the moment
/// the drop was queued. Without that refresh, a rapid second drop on a task
/// whose prior same-task save was still running could sit queued behind
/// `task_write_locks`' lock for a while before its own save even begins —
/// counting that queue wait against this same 8s budget could time out (and
/// visibly snap back) an overlay entry whose save hadn't even started yet.
const PENDING_MOVE_TIMEOUT: Duration = Duration::from_secs(8);

/// One-shot "landing flash" duration once a `pending_moves` entry resolves
/// as a success — kept short and subtle per the task-42 design brief, not a
/// sustained highlight.
const LANDING_FLASH_DURATION: Duration = Duration::from_millis(700);

/// How often `resolve_pending_moves` asks for a repaint while something is
/// pending or flashing (F5, post-review revision) — frequent enough that
/// the 700ms landing flash reads as a fade (a couple of visible steps) not
/// a single static frame, nowhere near frequent enough to matter for
/// battery/CPU given it only runs during an active drag-drop's brief
/// window. Also, incidentally but usefully, comfortably above
/// `egui_kittest`'s default `step_dt` (250ms, `HarnessBuilder`) — see
/// `render_board`'s own note on why that specific margin matters for every
/// kittest in this crate that drives a real drop.
///
/// That margin is only 50ms (300ms vs. 250ms `step_dt`) — brittle but
/// loud: if a future change shrinks this constant to within `step_dt`
/// again, `Context::request_repaint_after`'s own `predicted_dt` subtraction
/// (see `render_board`'s note) will collapse the requested delay back to
/// effectively zero under kittest specifically, and every drag/drop test in
/// this crate that calls plain `Harness::run` will start failing with
/// "exceeded max_steps" — a clear, immediate signal, not a silent
/// regression, but worth knowing the margin is this tight before touching
/// this value.
const LANDING_FLASH_REPAINT_INTERVAL: Duration = Duration::from_millis(300);

/// Column order: the shared status vocabulary (owner UX pass, 2026-08-05),
/// scoped to whichever projects are currently in view. Declaring a status
/// in a project's `config.yml` is enough to earn it a column even with zero
/// tasks in it right now — a repo-specific column like Icebox shouldn't
/// only appear once someone happens to file something there.
fn column_order(app: &HiveApp, snap: &Snapshot) -> Vec<String> {
    let scoped = scoped_projects(app, snap);
    ordered_status_vocabulary(scoped.iter().map(|row| &row.project))
}

pub(super) fn render_board(
    app: &mut HiveApp,
    ui: &mut egui::Ui,
    snap: &Snapshot,
    tasks: Vec<TaskRow<'_>>,
    pending: &mut Pending,
) {
    // task-42: resolve every in-flight drag-drop's own completion before
    // any column buckets a single card — see `resolve_pending_moves`'s own
    // doc. It requests a bounded, non-zero-delay repaint
    // (`LANDING_FLASH_REPAINT_INTERVAL`) while anything is pending/
    // flashing. `egui_kittest::Harness::run`'s settle loop (`try_run`'s own
    // source) only keeps stepping on a *zero-delay* repaint request —
    // `repaint_delay != Duration::ZERO` is its settle condition — and a
    // non-zero request only actually reaches `try_run` as non-zero if it
    // clears kittest's own `predicted_dt` subtraction inside `Context::
    // request_repaint_after` (`delay.saturating_sub(predicted_frame_time)`,
    // egui's own source), which for the *test* harness means clearing its
    // default `step_dt` (250ms, `HarnessBuilder`) specifically, not just
    // being "non-zero" in the abstract. `LANDING_FLASH_REPAINT_INTERVAL`
    // (300ms) clears that margin, so the practical implication holds
    // crate-wide: while any `pending_moves`/`landing_flash` entry exists,
    // plain `harness.run()` still settles — returning after exactly one
    // frame — everywhere a kittest drives a real Board drop, no
    // `Harness::step`/`run_steps` workaround needed. (Post-review
    // correction, N4: an earlier revision used a repaint interval short
    // enough to collapse to zero here, which genuinely did make `run()`
    // spin — the fix was the interval, not avoiding `run()`.)
    resolve_pending_moves(app, snap, ui.ctx());

    // TASK-26: keeps bulk_selected_tasks consistent with whatever's
    // currently visible — same per-frame call `list::render_task_workspace`
    // already makes, since the two lenses share `bulk_selected_tasks`.
    selection::retain_visible_bulk_selection(app, &tasks);
    render_bulk_selection_bar(app, ui);

    let columns = column_order(app, snap);
    let show_repo = app.backlog_view.selected_project.is_none();

    egui::ScrollArea::horizontal()
        .id_salt("backlog_board")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.horizontal_top(|ui| {
                for column_status in &columns {
                    render_column(app, ui, &tasks, column_status, show_repo, pending);
                }
            });
        });
}

/// Resolve every `pending_moves` entry against its *own* save's completion
/// report (`app.board_move_outcomes`, written by `HiveApp::
/// spawn_board_move_save`), refresh `queued_at` for any entry whose own
/// save has just started (`app.board_move_started` — N9), and expire any
/// `landing_flash` entry whose one-shot window has elapsed. The only place
/// any of the three is mutated outside a drop itself.
///
/// Drains `board_move_outcomes` once per frame (there's nothing to gain by
/// leaving a resolved outcome sitting in the shared map for a later frame
/// to look at again). An entry in `pending_moves` resolves only when a
/// drained outcome's `generation` matches that entry's own `generation`
/// exactly — a stale outcome for a generation `apply_drop` has since
/// superseded (a later drop on the same task) is recognized and discarded,
/// never used to resolve the *newer* entry (see `PendingBoardMove::
/// generation`'s doc for why the first version of this function got this
/// wrong by resolving off any cache reload instead). On a match: success
/// moves the key into `landing_flash` for one subtle flash frame-range;
/// either way the entry leaves `pending_moves`, so a loss silently and
/// immediately falls back to rendering the task's real status (the
/// rollback AC #2 asks for needs no separate code path — it falls out of
/// the overlay simply no longer applying).
///
/// `PENDING_MOVE_TIMEOUT` is a fallback exit for the one case an outcome
/// can't signal: the save thread never reporting one at all (e.g. a panic).
/// Without it a card could claim a status the real data never confirmed,
/// indefinitely — worse than a rare card that quietly reverts after a
/// bounded wait with no outcome ever having landed. On timeout, `snap` (this
/// frame's cache snapshot) is consulted as a best-effort courtesy check —
/// if the real data happens to already show the target status by then, the
/// move still gets its landing flash instead of a spurious snap-back.
///
/// Requests a bounded repaint while anything is still pending or flashing,
/// so the landing flash actually animates instead of painting one static
/// frame — see `render_board`'s own call site for the interval choice and
/// why it stays compatible with (rather than defeating) `egui_kittest::
/// Harness::run`'s settle loop.
fn resolve_pending_moves(app: &mut HiveApp, snap: &Snapshot, ctx: &egui::Context) {
    let now = Instant::now();
    let outcomes = std::mem::take(&mut *app.board_move_outcomes.lock().unwrap());

    // N9: refresh `queued_at` for any entry whose own save has just
    // reported starting (lock acquired, subprocess about to run) — see
    // `HiveApp::board_move_started`'s doc. Before the `retain` below so its
    // timeout check reads the refreshed value, not the drop-time one.
    let started = std::mem::take(&mut *app.board_move_started.lock().unwrap());
    for (key, generation) in started {
        if let Some(mv) = app.backlog_view.pending_moves.get_mut(&key) {
            if mv.generation == generation {
                mv.queued_at = now;
            }
        }
    }

    let mut landed: Vec<BacklogTaskKey> = Vec::new();
    app.backlog_view.pending_moves.retain(|key, mv| {
        if let Some(outcome) = outcomes.get(key) {
            if outcome.generation == mv.generation {
                if outcome.success {
                    landed.push(key.clone());
                }
                return false; // this generation's own save resolved
            }
            // A stale outcome for a generation this key's entry has since
            // moved past (a later drop superseded it) — not this entry's
            // business; keep waiting for the *current* generation's own
            // completion (or the timeout fallback below).
        }
        let timed_out = now.duration_since(mv.queued_at) > PENDING_MOVE_TIMEOUT;
        if !timed_out {
            return true; // still genuinely in flight
        }
        // Best-effort only (see the doc above) — no outcome ever arrived,
        // so fall back to whatever `snap` currently shows for this task.
        let succeeded = snap.project(&key.0).is_some_and(|project| {
            project
                .project
                .tasks
                .iter()
                .any(|t| t.id == key.1 && t.status.eq_ignore_ascii_case(&mv.target_status))
        });
        if succeeded {
            landed.push(key.clone());
        }
        false
    });
    for key in landed {
        app.backlog_view.landing_flash.insert(key, now);
    }
    app.backlog_view
        .landing_flash
        .retain(|_, started| now.duration_since(*started) <= LANDING_FLASH_DURATION);

    // Bounded: only while something is actually pending or flashing, and
    // egui coalesces repeated `request_repaint_after` calls to the soonest
    // one rather than stacking them, so this doesn't runaway. See the
    // kittest compatibility note on `render_board`'s call site.
    if !app.backlog_view.pending_moves.is_empty() || !app.backlog_view.landing_flash.is_empty() {
        ctx.request_repaint_after(LANDING_FLASH_REPAINT_INTERVAL);
    }
}

/// TASK-26 (owner-requested UX): the same "N selected · Clear" indicator
/// `list::render_task_sort_controls` shows, since Board shares the identical
/// `bulk_selected_tasks` state. Its own row rather than folded into an
/// existing one — Board has no sort/toolbar row of its own to attach to.
fn render_bulk_selection_bar(app: &mut HiveApp, ui: &mut egui::Ui) {
    let selected_count = app.backlog_view.bulk_selected_tasks.len();
    if selected_count == 0 {
        return;
    }
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(format!("{selected_count} selected")).color(theme::weak_text()),
        );
        ui.label(
            egui::RichText::new("· right-click a card for bulk actions")
                .small()
                .color(theme::muted_text()),
        );
        if ui
            .small_button("Clear")
            .on_hover_text("Clear selected tasks")
            .clicked()
        {
            app.backlog_view.bulk_selected_tasks.clear();
            app.backlog_view.bulk_selection_anchor = None;
        }
    });
    ui.add_space(4.0);
}

const COLUMN_WIDTH: f32 = 260.0;

fn render_column(
    app: &mut HiveApp,
    ui: &mut egui::Ui,
    all_visible: &[TaskRow<'_>],
    column_status: &str,
    show_repo: bool,
    pending: &mut Pending,
) {
    let column_tasks: Vec<&TaskRow<'_>> = all_visible
        .iter()
        .filter(|row| card_shows_in_column(app, row, column_status))
        .collect();

    ui.vertical(|ui| {
        ui.set_width(COLUMN_WIDTH);
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(column_status).strong());
            ui.label(
                egui::RichText::new(format!("{}", column_tasks.len())).color(theme::muted_text()),
            );
        });
        ui.separator();

        // `dnd_drop_zone` ignores the fill on the `Frame` it's handed and
        // always paints `visuals().widgets.{inactive,active}.bg_fill`
        // instead (see its source — it overwrites `frame.frame.fill` right
        // before painting). Overriding those two fields on this column's
        // child `Ui` is the only way to actually land our tuned `faint_bg`
        // instead of stock egui's default widget gray, which is what a
        // "No tasks" label would otherwise render against.
        //
        // `.active` is specifically the state `dnd_drop_zone` swaps in when
        // a drag carrying a compatible payload is hovering *this* column
        // (`is_anything_being_dragged && can_accept_what_is_being_dragged &&
        // response.contains_pointer()` — its own source). Pointing `.active`
        // at the *same* `faint_bg()` as `.inactive` (as this used to) is
        // exactly why a drag hovering a column produced no visible feedback
        // at all (task-42, AC #3) — `.active` now gets its own themed
        // accent fill/stroke instead.
        ui.visuals_mut().widgets.inactive.bg_fill = theme::faint_bg();
        ui.visuals_mut().widgets.active.bg_fill = theme::drop_target_fill();
        ui.visuals_mut().widgets.active.bg_stroke = theme::drop_target_stroke();
        let frame = egui::Frame::default().inner_margin(4.0);
        let (_, dropped) = ui.dnd_drop_zone::<BacklogTaskKey, ()>(frame, |ui| {
            ui.set_min_height(120.0);
            egui::ScrollArea::vertical()
                .id_salt(format!("backlog_board_col_{column_status}"))
                .max_height(ui.available_height().max(200.0))
                .show(ui, |ui| {
                    for row in &column_tasks {
                        render_strip(app, ui, row, all_visible, show_repo, pending);
                        ui.add_space(4.0);
                    }
                    if column_tasks.is_empty() {
                        ui.label(egui::RichText::new("No tasks").color(theme::muted_text()));
                    }
                });
        });

        if let Some(dropped_key) = dropped {
            apply_drop(app, all_visible, &dropped_key, column_status, ui.ctx());
        }
    });
}

/// The status a card currently *renders under*: `pending_moves`' target
/// status if an optimistic move for this task is in flight, else the task's
/// real status. `apply_drop`'s no-op guard uses this too (F2, post-review
/// fix) — comparing a drop's target against the task's *real* status let a
/// drag back out of an in-flight optimistic column silently do nothing,
/// since the real status hadn't changed yet.
///
/// Iterates `pending_moves` (never a `HashMap` lookup keyed by a freshly
/// cloned `(PathBuf, String)`) — deliberately: `pending_moves` is empty the
/// overwhelming majority of frames (no drag in flight) and realistically
/// holds at most a handful of entries even mid-drag, while this runs once
/// per row *per column*, every frame, for every visible card. A `HashMap::
/// get` keyed by `row.key()` would allocate that key on every one of those
/// calls (F4, post-review fix — the first version's doc claimed no
/// allocation while doing exactly that); scanning the tiny overlay instead
/// is both allocation-free and cheaper in the common (empty) case.
fn card_shows_in_column(app: &HiveApp, row: &TaskRow<'_>, column_status: &str) -> bool {
    if !app.backlog_view.pending_moves.is_empty() {
        if let Some((_, mv)) = app
            .backlog_view
            .pending_moves
            .iter()
            .find(|(key, _)| key.0 == row.project.key && key.1 == row.task.id)
        {
            return mv.target_status.eq_ignore_ascii_case(column_status);
        }
    }
    row.task.status.eq_ignore_ascii_case(column_status)
}

/// `dropped_key`'s card was released over `column_status`'s drop zone.
///
/// The no-op guard (F2, post-review fix) compares `column_status` against
/// the card's *effective* status, not its real one — this makes two drags
/// behave correctly that the first version got wrong:
/// - dropping back onto the same column a pending move is already headed
///   for is recognized as "already there" and skipped, instead of queuing a
///   redundant second subprocess for the same target;
/// - dragging a card back out of its own in-flight destination column (to
///   its real, origin status, or to any other column) is recognized as a
///   genuine new move — queued exactly like any other drop, which is also
///   what makes it "cancel" the appearance of the old one: the new
///   `PendingBoardMove` overwrites the old entry outright (same key), so
///   the card visually snaps to the new target this same frame, and
///   `HiveApp::task_write_locks` (see its doc) ensures the on-disk write
///   this produces is the one that ends up sticking.
fn apply_drop(
    app: &mut HiveApp,
    tasks: &[TaskRow<'_>],
    dropped_key: &BacklogTaskKey,
    column_status: &str,
    ctx: &egui::Context,
) {
    let Some(row) = tasks.iter().find(|row| &row.key() == dropped_key) else {
        return;
    };
    if card_shows_in_column(app, row, column_status) {
        return;
    }
    if !(row.task.editable() && row.project.project.cli_available()) {
        app.backlog_status
            .set(format!("{} is read-only; drag ignored", row.task.id));
        return;
    }
    let generation = app.backlog_view.next_move_generation;
    app.backlog_view.next_move_generation += 1;
    // task-42 AC #1: written synchronously, this same frame — see the
    // module doc's "Optimistic move + drop feedback" section for how
    // `render_column` reads it back before `spawn_board_move_save`'s CLI
    // subprocess (spawned below) ever resolves. Overwrites any prior entry
    // for this key outright — see this function's own doc for why that's
    // exactly the supersede behavior a second drop on the same task needs.
    app.backlog_view.pending_moves.insert(
        dropped_key.clone(),
        PendingBoardMove {
            target_status: column_status.to_string(),
            generation,
            queued_at: Instant::now(),
        },
    );
    app.backlog_status
        .set(format!("moving {} to {column_status}", row.task.id));
    // N10: calls `spawn_board_move_save` directly rather than going through
    // the `Pending`/`apply_pending` seam every other mutation in this
    // module uses (`pending.save = Some(...)`, drained after rendering) —
    // deliberately, not an oversight: `generation` must be stamped and
    // handed to the save in the same synchronous step that inserts the
    // overlay entry above, and `Pending` only carries a `(root, id, patch)`
    // tuple with no room for that pairing.
    app.spawn_board_move_save(
        row.project.key.clone(),
        row.task.id.clone(),
        BacklogTaskPatch {
            status: Some(column_status.to_string()),
            ..Default::default()
        },
        dropped_key.clone(),
        generation,
        ctx,
    );
}

/// One "flight strip": a repo-colored rail, id/title, priority, and AC
/// progress. Draggable when the task is CLI-editable; otherwise a plain,
/// non-interactive frame with the same layout so the board doesn't jump
/// around depending on editability.
///
/// TASK-29 (owner-reported live regression, 2026-08-05): card clicks and
/// the bulk-select checkbox never registered in the real app — this was a
/// genuine defect, not a kittest harness limitation as TASK-24/26 first
/// concluded (owner confirmed dead clicks against the live 0.31 build).
/// Root cause, confirmed by reading egui 0.31.1's own source
/// (`hit_test.rs::hit_test_on_close`): a retroactive `resp.interact(..)`
/// call — or `Ui::dnd_drag_source`'s own internal `Sense::drag()`-only
/// wrapper widget — registered *after* a card's content already painted
/// its own smaller widgets (the checkbox), and covering (containing)
/// their rect, always wins egui's hit-test tie-break for anything inside
/// it (`should_prioritize_hits_on_back` explicitly declines to protect a
/// widget that's "fully occluded" by a larger one registered on top). And
/// when that winning widget senses *only* drag — exactly what
/// `dnd_drag_source` auto-generates — egui explicitly discards any click
/// underneath it: "ignore the click-widget, because it would be confusing
/// if clicking a drag-widget would actually click something else below
/// it" (hit_test.rs's own comment). That's what silently ate every card
/// click *and* the checkbox's own click sense, on editable (drag-wrapped)
/// cards; non-editable cards had the milder version of the same bug (the
/// checkbox lost ties to the card-wide click interact for the same
/// "larger widget registered after, containing a smaller one" reason,
/// just without the drag-only widget's harsher "discard the click
/// entirely" rule on top).
///
/// Fix, two structural changes:
/// 1. The bulk-select checkbox is a **sibling** of the "click to open,
///    drag to move" region (`content_rect`, captured from its own
///    `ui.scope`), not nested inside it — their rects never overlap, so
///    there is no tie for egui to resolve either way.
/// 2. "Click to open" and "drag to reorder" are **one** widget
///    (`Sense::click_and_drag()`), not two competing ones. A single widget
///    sensing both lets egui's own press/release-without-movement vs.
///    movement-past-threshold logic (`PointerState::is_decidedly_dragging`)
///    disambiguate which happened, instead of a drag-only layer shadowing
///    everything below it. This rules out `Ui::dnd_drag_source` — it
///    always creates that second, drag-only widget — so the drag payload
///    is set manually via `Response::dnd_set_drag_payload`, and the
///    floating "ghost" card shown mid-drag is reimplemented by hand in
///    `render_drag_ghost`, mirroring `dnd_drag_source`'s own dragging
///    branch line-for-line (still safe to mirror — only the *non*-dragging
///    branch, which layers the extra drag-only widget, is the culprit).
///
/// Trade-off: a drag can only be started by pressing on the card body, not
/// directly on the checkbox — acceptable, since a checkbox that also
/// drag-initiates would be surprising UX regardless.
fn render_strip(
    app: &mut HiveApp,
    ui: &mut egui::Ui,
    row: &TaskRow<'_>,
    all_visible: &[TaskRow<'_>],
    show_repo: bool,
    pending: &mut Pending,
) {
    let key = row.key();
    let editable = row.task.editable() && row.project.project.cli_available();
    let bulk_selected = app.backlog_view.bulk_selected_tasks.contains(&key);
    let selected = app.backlog_view.selected_task.as_ref() == Some(&key) || bulk_selected;
    // TASK-26: shift-range select needs the full visible order, same
    // "flatten once, reuse per click" shape list.rs's own row rendering
    // uses for its `visible_keys` parameter.
    let visible_keys: Vec<BacklogTaskKey> = all_visible.iter().map(TaskRow::key).collect();
    let card_id = egui::Id::new(("backlog_board_strip", &key));

    if editable && ui.ctx().is_being_dragged(card_id) {
        render_drag_ghost(ui, card_id, row, show_repo, selected, bulk_selected);
        return;
    }

    let motion = card_motion(app, &key);
    let (checkbox_resp, checked_now, content_rect) =
        paint_card(ui, row, show_repo, selected, bulk_selected, motion);
    if checkbox_resp.changed() {
        // TASK-26 (owner-requested UX): bulk-select checkbox, reusing the
        // exact same `selection` state machine list.rs's row checkbox
        // drives (`bulk_selected_tasks`/`bulk_selection_anchor` are shared
        // across lenses, not per-lens state) — shift toggles range-select
        // the same way.
        let shift = ui.input(|input| input.modifiers.shift);
        if shift {
            selection::select_bulk_task_range(app, &visible_keys, key.clone());
        } else {
            selection::set_bulk_task_selected(app, key.clone(), checked_now);
        }
    }

    let sense = if editable {
        egui::Sense::click_and_drag()
    } else {
        egui::Sense::click()
    };
    // N5 (post-review, bounded investigation — timeboxed, documented rather
    // than fixed): this retroactive `ui.interact(content_rect, ...)` region
    // (TASK-29's own doc above explains why it exists and why it can't be
    // restructured casually) gets no explicit accessible label, so
    // `accesskit_consumer::Node::labelled_by` auto-derives one from its
    // descendant Label/Image nodes for interactive-role widgets with none
    // set (confirmed by reading its source, not guessed:
    // `accesskit_consumer-0.25.0/src/node.rs`'s `labelled_by`, the
    // `FromDescendants` branch). With `CardMotion::Saving` active, the
    // extra "saving…" label inside this same region changes which
    // descendant(s) that auto-derivation picks up, which empirically makes
    // the card's own title fail an *exact*-match accessibility query
    // (`kittest`'s `by().label(...)`, which excludes a node that's the
    // `labelled_by` target of another matched node) — the same query a
    // real screen reader's accessible-name resolution would also be
    // affected by. That's a genuine, not-yet-fixed a11y regression for
    // in-flight cards specifically, not just a test-query quirk: a screen
    // reader could plausibly announce this region by some other descendant
    // text instead of the title while a card is Saving. Restructuring this
    // region to carry an explicit label (e.g. `.on_hover_text` doesn't set
    // one, but egui's `Response` does have label-setting affordances) is
    // the real fix; out of scope for this bounded pass — kittest tests that
    // need to locate a card while `CardMotion::Saving` is active query by
    // its stable task-id label instead (see `leftmost_bounds`'s doc in
    // `tests/backlog_controls.rs`), which side-steps the same underlying
    // issue rather than actually fixing it.
    let mut interacted = ui
        .interact(content_rect, card_id, sense)
        .on_hover_text("Show details in the rail");
    if editable {
        interacted = interacted.on_hover_cursor(egui::CursorIcon::Grab);
        interacted.dnd_set_drag_payload(key.clone());
    }
    if interacted.clicked() {
        // TASK-24 (owner-requested UX), superseded by the 2026-08-05 rail
        // pass: a Board card click used to jump to the List lens just to
        // reach its detail pane. Now the persistent detail rail
        // (`rail::render_detail_rail`) shows any selected task's detail
        // regardless of lens, so selecting is enough — no lens switch.
        app.backlog_view.selected_task = Some(key.clone());
        app.backlog_view.editor.loaded_key = None;
    }
    // TASK-26: right-click bulk actions, reusing list::
    // render_task_context_menu exactly as list.rs's own row does — same
    // pattern as the List lens's own right-click menu.
    if interacted.secondary_clicked() {
        selection::focus_context_selection(app, key.clone());
    }
    interacted.context_menu(|ui| {
        list::render_task_context_menu(app, ui, row, all_visible, pending);
    });
}

/// Drag-drop-driven visual state for one card (task-42), on top of the
/// pre-existing `selected` treatment. `Normal` outside any pending/just-
/// landed window. `Saving` for as long as this task's key is in
/// `pending_moves` — dimmed frame, muted title, plus a small pulsing dot and
/// "saving…" label so the in-flight state reads unambiguously (not just a
/// subtler shade of normal). `Landing(progress)` is the one-shot flash right
/// after a `pending_moves` entry resolves as a success, `progress` running
/// 0.0 (flash start) to 1.0 (flash end) as `resolve_pending_moves` ages the
/// `landing_flash` entry — `paint_card` fades a green border out over it.
#[derive(Clone, Copy, PartialEq)]
enum CardMotion {
    Normal,
    Saving,
    Landing(f32),
}

/// Derive `row`'s current `CardMotion` from `app.backlog_view`'s two task-42
/// overlays. Landing takes priority over Saving — they're mutually
/// exclusive in practice (`resolve_pending_moves` only ever populates
/// `landing_flash` for a key it simultaneously removes from
/// `pending_moves`), but Landing is the more specific state if that ever
/// changed.
fn card_motion(app: &HiveApp, key: &BacklogTaskKey) -> CardMotion {
    if let Some(started) = app.backlog_view.landing_flash.get(key) {
        let progress = started.elapsed().as_secs_f32()
            / LANDING_FLASH_DURATION.as_secs_f32().max(f32::EPSILON);
        return CardMotion::Landing(progress.clamp(0.0, 1.0));
    }
    if app.backlog_view.pending_moves.contains_key(key) {
        return CardMotion::Saving;
    }
    CardMotion::Normal
}

/// Paints one card's frame, checkbox, and content — pure function of its
/// input, no `HiveApp` access, so it can be reused unchanged for both the
/// normal in-place render and the mid-drag floating ghost
/// (`render_drag_ghost`, always `CardMotion::Normal` — a card being actively
/// dragged can't simultaneously be mid-save). Returns `(checkbox_response,
/// checkbox_checked_after_paint, content_rect)`: `content_rect` is
/// deliberately the "dot + vertical" sub-area only, excluding the
/// checkbox, so the caller's click/drag interact call never overlaps the
/// checkbox's own (see `render_strip`'s doc for why that matters).
fn paint_card(
    ui: &mut egui::Ui,
    row: &TaskRow<'_>,
    show_repo: bool,
    selected: bool,
    bulk_selected: bool,
    motion: CardMotion,
) -> (egui::Response, bool, egui::Rect) {
    // The fill is always `theme::card_bg()` — every text color rendered
    // inside a strip is tuned against that exact card color (see
    // `theme.rs`'s palette doc). Not `ui.visuals().extreme_bg_color`: the
    // owner UX pass repointed that egui slot to input fields instead, so
    // reading it here would give cards the wrong (recessed, not raised)
    // tone. Selection is a border color change instead of a translucent
    // overlay: layering `visuals().selection.bg_fill` (untuned, stock egui)
    // at partial alpha over the card produced a muddy composite that
    // failed WCAG AA on the dark theme — a stroke can't create that
    // problem since the audit only measures fills and text, never strokes.
    //
    // `Landing` reuses that same "stroke, not fill" reasoning for its own
    // flash: a green border that fades out over `LANDING_FLASH_DURATION`
    // (via `theme::scale_alpha`), rather than a translucent fill wash that
    // would risk the identical WCAG problem `selected`'s doc above already
    // ruled out a fill-based treatment for.
    let stroke = match motion {
        CardMotion::Landing(progress) => {
            egui::Stroke::new(2.0, theme::scale_alpha(theme::green(), 1.0 - progress))
        }
        _ if selected => egui::Stroke::new(2.0, theme::sky()),
        _ => ui.visuals().widgets.noninteractive.bg_stroke,
    };
    let mut frame = egui::Frame::default()
        .fill(theme::card_bg())
        .stroke(stroke)
        .corner_radius(3.0)
        .inner_margin(egui::Margin::symmetric(8, 6));
    if motion == CardMotion::Saving {
        // Dims the frame's own fill/stroke/shadow only (egui's own
        // `Frame::multiply_with_opacity` — it never touches the *content*
        // painted inside), so the card visibly recedes while its text stays
        // legible; the title's color switches to `muted_text()` below for
        // the matching "this isn't final yet" read on the text itself.
        frame = frame.multiply_with_opacity(0.55);
    }
    let mut checked = bulk_selected;
    let mut content_rect = egui::Rect::NOTHING;
    let checkbox_resp = frame
        .show(ui, |ui| {
            ui.set_width(COLUMN_WIDTH - 16.0);
            ui.horizontal(|ui| {
                let checkbox = ui
                    .add_sized([20.0, 18.0], egui::Checkbox::without_text(&mut checked))
                    .on_hover_text("Select task for bulk actions");
                let content_resp = ui
                    .scope(|ui| {
                        let _ =
                            theme::painted_dot(ui, theme::repo_rail_color(&row.project.repo_name));
                        ui.vertical(|ui| {
                            if show_repo {
                                ui.label(
                                    egui::RichText::new(&row.project.repo_name)
                                        .small()
                                        .color(theme::muted_text()),
                                );
                            }
                            ui.label(
                                egui::RichText::new(&row.task.id)
                                    .monospace()
                                    .small()
                                    .color(theme::muted_text()),
                            );
                            let title_color = if motion == CardMotion::Saving {
                                theme::muted_text()
                            } else {
                                ui.visuals().text_color()
                            };
                            ui.label(
                                egui::RichText::new(&row.task.title)
                                    .strong()
                                    .small()
                                    .color(title_color),
                            );
                            ui.horizontal(|ui| {
                                ui.label(
                                    egui::RichText::new(format::priority_title(&row.task.priority))
                                        .small()
                                        .color(format::priority_color(&row.task.priority)),
                                );
                                if !row.task.acceptance_criteria.is_empty() {
                                    ui.label(
                                        egui::RichText::new(format!(
                                            "{}/{}",
                                            row.task.acceptance_done_count(),
                                            row.task.acceptance_criteria.len()
                                        ))
                                        .small()
                                        .color(theme::muted_text()),
                                    );
                                }
                                // task-18 lamp-language marker — same
                                // rationale as the List lens's blocked pill.
                                if !row.task.is_done()
                                    && switchbard_core::is_blocked(row.task, &row.project.project)
                                {
                                    ui.label(
                                        egui::RichText::new("blocked")
                                            .small()
                                            .strong()
                                            .color(theme::warn_orange()),
                                    );
                                }
                                dispatch_ui::render_dispatch_pill(
                                    ui,
                                    &dispatch_ui::dispatch_state(row.task),
                                );
                                // task-42 AC #1: a clear, queryable in-flight
                                // treatment — the dimmed frame alone reads as
                                // "disabled" as easily as "in progress", so
                                // this spells it out. `theme::painted_dot_
                                // pulse`'s own `request_repaint_after`
                                // (500ms, `PULSE_FRAME_MS`) is safely above
                                // kittest's default `step_dt` too (see
                                // `render_board`'s note on that margin), so
                                // — post-review correction, same class as
                                // N4 — it does not in fact break the plain
                                // `harness.run()` idiom, and the "live
                                // activity" pulse language this app already
                                // uses elsewhere (listener dots) is the
                                // better match for a real "spinner" than a
                                // static one.
                                if motion == CardMotion::Saving {
                                    theme::painted_dot_pulse(ui, theme::sky(), 1);
                                    ui.label(
                                        egui::RichText::new("saving…")
                                            .small()
                                            .italics()
                                            .color(theme::muted_text()),
                                    );
                                }
                            });
                            render_labels_and_age(ui, row.task);
                        });
                    })
                    .response;
                content_rect = content_resp.rect;
                checkbox
            })
            .inner
        })
        .inner;
    (checkbox_resp, checked, content_rect)
}

/// The floating "ghost" shown while a card is mid-drag — repaints the same
/// visual content onto an `Order::Tooltip` layer near the pointer,
/// mirroring `Ui::dnd_drag_source`'s own dragging branch (not reused
/// directly — see `render_strip`'s doc for why `dnd_drag_source` itself
/// can't be used for the non-dragging path). `Order::Tooltip` responses
/// are always inert ("anything with `Order::Tooltip` always gets an empty
/// Response" — egui's own doc comment), so painting the checkbox here is
/// purely cosmetic; it can't receive input mid-drag, which is fine since
/// nothing needs to change on the ghost itself.
fn render_drag_ghost(
    ui: &mut egui::Ui,
    card_id: egui::Id,
    row: &TaskRow<'_>,
    show_repo: bool,
    selected: bool,
    bulk_selected: bool,
) {
    egui::DragAndDrop::set_payload(ui.ctx(), row.key());
    let layer_id = egui::LayerId::new(egui::Order::Tooltip, card_id);
    let response = ui
        .scope_builder(egui::UiBuilder::new().layer_id(layer_id), |ui| {
            paint_card(
                ui,
                row,
                show_repo,
                selected,
                bulk_selected,
                CardMotion::Normal,
            );
        })
        .response;
    if let Some(pointer_pos) = ui.ctx().pointer_interact_pos() {
        let delta = pointer_pos - response.rect.center();
        ui.ctx()
            .transform_layer_shapes(layer_id, egui::emath::TSTransform::from_translation(delta));
    }
}

/// Labels and a humanized age (webview kanban card parity, QA parity matrix
/// row "Kanban card: labels"/"Kanban card: age" — previously a LOW gap).
/// Skips the whole line when there's nothing to show, same convention as
/// `dispatch_ui::render_dispatch_pill`'s `NotFlagged` no-op, so an
/// unlabeled/undated card doesn't paint an empty row.
fn render_labels_and_age(ui: &mut egui::Ui, task: &BacklogTask) {
    let age = card_age(task);
    if task.labels.is_empty() && age.is_none() {
        return;
    }
    ui.horizontal(|ui| {
        if !task.labels.is_empty() {
            ui.label(
                egui::RichText::new(task.labels.join(", "))
                    .small()
                    .color(theme::muted_text()),
            );
        }
        if let Some(age) = age {
            ui.label(egui::RichText::new(age).small().color(theme::muted_text()));
        }
    });
}

/// Prefers `updated_date` (the webview's card age reflects last activity,
/// not creation) and falls back to `created_date` for a task never edited
/// since creation. `None` for a task with neither date parseable — the card
/// just omits the age rather than showing a placeholder.
fn card_age(task: &BacklogTask) -> Option<String> {
    task.updated_date
        .as_deref()
        .or(task.created_date.as_deref())
        .and_then(parse_backlog_datetime_unix)
        .map(humanize_age)
}
