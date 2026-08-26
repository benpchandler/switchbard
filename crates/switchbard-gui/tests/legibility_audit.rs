//! Legibility audit — walks every *painted* text run in the real Switchbard
//! views and asserts it clears the legibility contract (`ui::legibility`).
//!
//! This is the machine-checkable half of UI review. Where `ui_views.rs` proves
//! the right widgets exist and `ui_snapshot.rs` is a human-reviewed pixel
//! baseline, this test proves a property no screenshot diff can: that no text
//! the user is meant to read is below the size floor or below WCAG AA contrast.
//!
//! How it works, from first principles and naive to current state:
//!   1. Mount each view through the same `egui_kittest` harness production uses.
//!   2. Read `harness.output().shapes` — the exact `epaint` draw list. Every
//!      `Shape::Text` carries a galley whose sections expose the *resolved*
//!      font size and color, so we measure what was actually painted, not what
//!      the source intended.
//!   3. Resolve the actual background behind each run from the draw list's
//!      filled rects (so white-on-red button text is measured against the
//!      button, not the panel), then check size + contrast against the floors.
//!
//! It does not know which call sites are wrong; it discovers them. A failure
//! prints every offending run, grouped by view, with text / size / contrast so
//! the fix is obvious.
//!
//! ## Disabled controls are exempt (WCAG 1.4.3)
//!
//! egui's `Ui::disable()` fades every shape's color 50% toward `Visuals::
//! fade_out_to_color()` — a fixed blend baked into the framework, not
//! something a palette choice can tune away, since it always pulls whatever
//! color the enabled widget would have painted halfway toward a background
//! tone. WCAG 2.1 Success Criterion 1.4.3 explicitly excludes this case:
//! "Text... that is part of an inactive user interface component has no
//! contrast requirement." So rather than chase an architecturally-unreachable
//! target, `audit_owned` looks up each run's containing node in the real
//! accessibility tree (the same tree `ui_views.rs` queries) and skips any
//! run whose node reports `is_disabled()`.

mod common;

use egui_kittest::kittest::NodeT;
use std::collections::BTreeMap;
use std::fmt::Write as _;

use std::path::PathBuf;

use common::{harness, seeded_app, REPO_NAME, REPO_PATH};
use eframe::egui::{epaint::Shape, Color32, Pos2, Rect};
use egui_kittest::kittest::{by, Queryable};
use egui_kittest::Harness;
use switchbard_core::config::ThemeChoice;
use switchbard_core::{
    AttributedListener, BacklogChecklistItem, BacklogProject, BacklogTask, BacklogTaskSource,
    LocalListener, DISPATCHED_LABEL, DISPATCHING_LABEL, DISPATCH_FAILED_LABEL, DISPATCH_LABEL,
};
use switchbard_gui::app::HiveApp;
use switchbard_gui::runtime::{BacklogLens, ViewTab};
use switchbard_gui::ui::legibility;

/// One painted run of text with a single resolved size + color.
struct TextRun {
    text: String,
    size: f32,
    /// Premultiplied color as painted (before compositing over the background).
    color: Color32,
    pos: Pos2,
    /// Position in the draw list. Used to tell whether a covering scrim was
    /// painted *after* this run, i.e. whether the run is behind a modal.
    seq: usize,
}

/// A run that failed the contract, with the perceived contrast and why.
struct Violation {
    view: String,
    text: String,
    size: f32,
    contrast: f64,
    pos: Pos2,
    reasons: Vec<&'static str>,
}

/// A filled rectangle from the draw list — a candidate background for any text
/// painted on top of it (button fills, selection highlights, card frames).
struct FilledRect {
    rect: Rect,
    fill: Color32,
    /// Position in the draw list; see [`TextRun::seq`].
    seq: usize,
}

/// Recursively pull `Shape::Text` runs and `Shape::Rect` fills out of the draw
/// list. egui sometimes nests shapes inside `Shape::Vec`, so we descend too.
fn collect(shape: &Shape, runs: &mut Vec<TextRun>, rects: &mut Vec<FilledRect>, seq: &mut usize) {
    match shape {
        Shape::Text(t) => {
            for section in &t.galley.job.sections {
                // Resolve the color exactly as the painter will: a PLACEHOLDER
                // section defers to the shape's fallback color, and
                // override_text_color (if set) wins for the glyphs.
                let raw = section.format.color;
                let resolved = if raw == Color32::PLACEHOLDER {
                    t.fallback_color
                } else {
                    raw
                };
                let mut color = t.override_text_color.unwrap_or(resolved);

                // Fold in any whole-shape opacity (egui scales in gamma space).
                if t.opacity_factor < 1.0 {
                    let o = t.opacity_factor;
                    let s = |c: u8| (c as f32 * o).round() as u8;
                    color = Color32::from_rgba_premultiplied(
                        s(color.r()),
                        s(color.g()),
                        s(color.b()),
                        s(color.a()),
                    );
                }

                let text = t
                    .galley
                    .job
                    .text
                    .as_str()
                    .get(section.byte_range.start.0..section.byte_range.end.0)
                    .unwrap_or_default();
                if text.trim().is_empty() {
                    continue;
                }
                *seq += 1;
                runs.push(TextRun {
                    text: text.to_string(),
                    size: section.format.font_id.size,
                    color,
                    pos: t.pos,
                    seq: *seq,
                });
            }
        }
        Shape::Rect(r) if r.fill.a() > 0 => {
            *seq += 1;
            rects.push(FilledRect {
                rect: r.rect,
                fill: r.fill,
                seq: *seq,
            });
        }
        Shape::Vec(shapes) => {
            for s in shapes {
                collect(s, runs, rects, seq);
            }
        }
        _ => {}
    }
}

/// The color a viewer actually sees behind `pos`: every filled rect that covers
/// the point, composited over the panel fill from largest (outermost) to
/// smallest (innermost). This is what makes the contrast check honest — white
/// button text is measured against the button's red fill, not the panel.
fn background_at(pos: Pos2, rects: &[FilledRect], base: Color32) -> Color32 {
    let mut covering: Vec<&FilledRect> = rects.iter().filter(|r| r.rect.contains(pos)).collect();
    covering.sort_by(|a, b| {
        let area = |r: Rect| (r.width() * r.height()) as f64;
        area(b.rect).total_cmp(&area(a.rect))
    });
    covering
        .into_iter()
        .fold(base, |bg, r| legibility::composite_over(r.fill, bg))
}

/// Rects that are a floating window's **drop shadow**: a pure-black
/// translucent fill egui paints under a `Window`/`Area` before the window
/// itself.
///
/// WCAG 1.4.3 exempts text that is part of an inactive component. Text lying
/// under an open dialog's shadow is background content the dialog has
/// deliberately pushed back — the same category, and the same reason, as the
/// disabled controls the module doc already exempts.
///
/// Deliberately narrow, because this is an exemption from a safety contract
/// and a loose rule here would silently stop auditing real text:
/// - the fill must be **pure black with partial alpha**, which is what
///   `Shadow::color` produces; a themed translucent fill (a hover highlight, a
///   selected row, a tinted card) is not a shadow and is not exempt;
/// - it only exempts runs painted **before** the shadow. Anything drawn after
///   it is on top of the dialog and stays fully in scope.
fn shadow_rects(rects: &[FilledRect]) -> Vec<&FilledRect> {
    rects
        .iter()
        .filter(|r| {
            let f = r.fill;
            f.a() > 0 && f.a() < 255 && f.r() == 0 && f.g() == 0 && f.b() == 0
        })
        .collect()
}

/// Every disabled node's bounds in `harness`'s accessibility tree, converted
/// to `egui::Rect` — see the module doc's "Disabled controls are exempt"
/// section for why this is the correct exemption rather than a gap.
fn disabled_rects(harness: &Harness<'_, HiveApp>) -> Vec<Rect> {
    harness
        .query_all(by())
        .filter(|node| node.accesskit_node().is_disabled())
        // `bounding_box()`, not `raw_bounds()`: the latter is the node's own
        // untransformed rect, while this exemption is compared against painted
        // screen coordinates. Under egui 0.36 panels nest inside a parent
        // `Ui`, so a non-identity transform is now the normal case and the two
        // no longer coincide — using the raw rect silently stopped exempting
        // controls inside a panel.
        .filter_map(|node| node.accesskit_node().bounding_box())
        .map(|r| {
            Rect::from_min_max(
                Pos2::new(r.x0 as f32, r.y0 as f32),
                Pos2::new(r.x1 as f32, r.y1 as f32),
            )
        })
        .collect()
}

/// Audit one rendered view against the legibility contract. `view` is owned
/// rather than `&'static str` because the theme-parameterized `views()`
/// formats each name with a `[Light]`/`[Dark]` suffix.
fn audit_owned(view: String, harness: &Harness<'_, HiveApp>) -> Vec<Violation> {
    let panel = harness.ctx.style_of(harness.ctx.theme()).visuals.panel_fill;
    let disabled = disabled_rects(harness);

    let mut runs = Vec::new();
    let mut rects = Vec::new();
    let mut seq = 0usize;
    for clipped in &harness.output().shapes {
        collect(&clipped.shape, &mut runs, &mut rects, &mut seq);
    }

    let shadows = shadow_rects(&rects);

    let mut violations = Vec::new();
    for run in runs {
        // WCAG 1.4.3 exempts inactive-component text — see the module doc.
        if disabled.iter().any(|r| r.contains(run.pos)) {
            continue;
        }
        // Same exemption, same clause: this run sits under a dialog's drop
        // shadow, i.e. it is background content. See `shadow_rects`.
        if shadows
            .iter()
            .any(|s| s.seq > run.seq && s.rect.contains(run.pos))
        {
            continue;
        }
        let bg = background_at(run.pos, &rects, panel);
        let perceived = legibility::composite_over(run.color, bg);
        let contrast = legibility::contrast_ratio(perceived, bg);

        let mut reasons = Vec::new();
        if run.size < legibility::MIN_FONT_POINTS {
            reasons.push("size");
        }
        if contrast < legibility::min_contrast_for(run.size) {
            reasons.push("contrast");
        }
        if !reasons.is_empty() {
            violations.push(Violation {
                view: view.clone(),
                text: run.text,
                size: run.size,
                contrast,
                pos: run.pos,
                reasons,
            });
        }
    }
    violations
}

fn snippet(s: &str) -> String {
    let one_line = s.replace('\n', "⏎").replace('\t', " ");
    if one_line.chars().count() > 44 {
        let truncated: String = one_line.chars().take(44).collect();
        format!("{truncated}…")
    } else {
        one_line
    }
}

/// Human-readable failure: counts up top, then each view with its distinct
/// offending runs (deduped, with an occurrence count and a sample position).
fn report(violations: &[Violation]) -> String {
    let mut s = String::new();
    writeln!(
        s,
        "\n{} text runs fail the legibility contract \
         (size floor {}pt, WCAG AA {}:1 normal / {}:1 large).",
        violations.len(),
        legibility::MIN_FONT_POINTS,
        legibility::MIN_CONTRAST_NORMAL,
        legibility::MIN_CONTRAST_LARGE,
    )
    .unwrap();

    let mut by_view: BTreeMap<&str, Vec<&Violation>> = BTreeMap::new();
    for v in violations {
        by_view.entry(v.view.as_str()).or_default().push(v);
    }

    for (view, items) in &by_view {
        writeln!(s, "\n  ── {} ({} runs) ──", view, items.len()).unwrap();

        // Dedup identical (text, size, reasons) runs; keep a count + a sample.
        let mut groups: BTreeMap<String, (usize, &Violation)> = BTreeMap::new();
        for v in items {
            let key = format!("{}|{:.1}|{}", snippet(&v.text), v.size, v.reasons.join("+"));
            let entry = groups.entry(key).or_insert((0, v));
            entry.0 += 1;
        }

        // Sort the display by reason then size so size-floor misses cluster.
        let mut rows: Vec<(usize, &Violation)> = groups.into_values().collect();
        rows.sort_by(|(_, a), (_, b)| {
            a.reasons
                .join("+")
                .cmp(&b.reasons.join("+"))
                .then(a.size.partial_cmp(&b.size).unwrap())
        });

        for (count, v) in rows {
            let times = if count > 1 {
                format!(" ×{count}")
            } else {
                String::new()
            };
            writeln!(
                s,
                "    [{:<13}] {:>4.1}pt  {:>4.1}:1  \"{}\"{}  (@{:.0},{:.0})",
                v.reasons.join("+"),
                v.size,
                v.contrast,
                snippet(&v.text),
                times,
                v.pos.x,
                v.pos.y,
            )
            .unwrap();
        }
    }
    s
}

/// Write a small markdown file at the seeded CLAUDE.md path so the detail
/// drawer renders a real preview body (the screenshot's small gray text) rather
/// than a "failed to read" message. Content is irrelevant to the audit — every
/// preview byte is painted at the same size — but it keeps the fixture faithful.
fn write_preview_fixture() {
    let path = std::path::Path::new(REPO_PATH).join("CLAUDE.md");
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(
        &path,
        "# Project Instructions\n\n\
         These are the effective instructions an agent reads in this repo.\n\n\
         - Prefer small, composable functions.\n\
         - Keep the validation boundary singular.\n",
    );
}

/// Seed one attributed listener so the audited frame is a *live* dashboard: a
/// running service paints its workspace row, and — crucially — the top bar's
/// "Kill all in filter" button is **enabled** rather than disabled.
///
/// Why this matters for the audit: egui fades disabled widgets by blending
/// their text toward the panel (`set_fade_to_color`), which is pixel-identical
/// to a genuinely low-contrast label. WCAG 1.4.3 exempts inactive controls, but
/// the draw list carries no "disabled" flag, so the audit can't tell them
/// apart. Rather than special-case it, we audit a realistic enabled state.
/// A Workspace harness with a *linked* worktree that is bulk-selected.
///
/// The selection tint and outline only paint in this state, and `seeded_app`
/// gives every harness a single worktree that is also the repo root — i.e. the
/// primary, which is deliberately never selectable. Without this fixture the
/// audit would pass over the selected row without ever measuring text against
/// it, which is exactly the kind of green that proves nothing.
fn seed_selected_linked_worktree(app: &mut HiveApp) {
    let linked = std::path::PathBuf::from("/tmp/switchbard-ui-test/demo-feature");
    app.worktrees
        .lock()
        .unwrap()
        .push(switchbard_core::WorktreeRef {
            repo_name: REPO_NAME.to_string(),
            path: linked.clone(),
            branch: Some("feat/selected".to_string()),
            head: "def5678".to_string(),
        });
    app.bulk_selected_worktrees.insert(linked);
}

fn seed_live_listener(app: &HiveApp) {
    app.state
        .lock()
        .unwrap()
        .listeners
        .push(AttributedListener {
            listener: LocalListener {
                pid: 4242,
                pgid: 4242,
                port: 5173,
                command_name: "vite".to_string(),
                cwd: Some(REPO_PATH.into()),
            },
            repo_name: Some(REPO_NAME.to_string()),
            worktree_path: Some(REPO_PATH.into()),
            worktree_branch: Some("main".to_string()),
        });
}

/// A Backlog task with real content in every task-15/16 surface: a
/// multi-element markdown description (exercises `egui_commonmark`
/// rendering, not just the "No description" empty branch), dependencies,
/// references, a milestone, and both checklists — so the audit actually
/// covers the parity work, not just the pre-existing views.
fn legibility_backlog_task() -> BacklogTask {
    BacklogTask {
        id: "TASK-1".to_string(),
        title: "Legibility fixture task".to_string(),
        status: "In Progress".to_string(),
        priority: "high".to_string(),
        assignees: vec!["ben".to_string()],
        // `DISPATCH_LABEL` puts this task — the one selected by default in
        // every List/Board harness below — in the Queued dispatch state, so
        // the detail pane's dispatch section (pill + button + confirm) is
        // part of the audited surface, not just the List/Board row pill.
        labels: vec!["demo".to_string(), DISPATCH_LABEL.to_string()],
        dependencies: vec!["TASK-2".to_string()],
        references: vec!["https://example.com/spec".to_string()],
        milestone: Some("v1".to_string()),
        parent: None,
        created_date: Some("2026-06-01 09:00".to_string()),
        updated_date: Some("2026-06-20 12:00".to_string()),
        description: "## Why\n\nThis exercises **CommonMark** rendering with a list:\n\n- first item\n- second item\n\nand a [link](https://example.com).".to_string(),
        implementation_plan: "Step one, then step two.".to_string(),
        implementation_notes: "Existing note text.".to_string(),
        final_summary: String::new(),
        acceptance_criteria: vec![BacklogChecklistItem {
            index: 1,
            checked: false,
            text: "Criterion renders".to_string(),
        }],
        definition_of_done: vec![BacklogChecklistItem {
            index: 1,
            checked: true,
            text: "DoD item renders".to_string(),
        }],
        source: BacklogTaskSource::Active,
        path: PathBuf::from(format!("{REPO_PATH}/backlog/tasks/task-1.md")),
    }
}

/// TASK-1's open dependency (task-18) — gives `legibility_backlog_task`'s
/// `dependencies: ["TASK-2"]` something real to resolve against, so the
/// "blocked" marker (List row, Board strip, detail header) actually renders
/// during the audit instead of silently no-oping on an unresolved id.
fn legibility_blocking_task() -> BacklogTask {
    let mut task = legibility_backlog_task();
    task.id = "TASK-2".to_string();
    task.title = "Blocking dependency".to_string();
    task.status = "To Do".to_string();
    task.dependencies = vec![];
    task.parent = None;
    task.path = PathBuf::from(format!("{REPO_PATH}/backlog/tasks/task-2.md"));
    task
}

/// A direct child of `legibility_backlog_task` (task-17) — exercises the
/// List lens's roll-up badge and, once expanded, its nested child row.
fn legibility_subtask() -> BacklogTask {
    let mut task = legibility_backlog_task();
    task.id = "TASK-3".to_string();
    task.title = "Subtask".to_string();
    task.status = "To Do".to_string();
    task.dependencies = vec![];
    task.parent = Some("TASK-1".to_string());
    task.path = PathBuf::from(format!("{REPO_PATH}/backlog/tasks/task-3.md"));
    task
}

/// TASK-1 (above) covers the Queued dispatch state. The other three label
/// states each get their own standalone task here so every pill text
/// ("DISPATCHING" / "DISPATCHED" / "DISPATCH FAILED") and, for the two with
/// distinct detail-pane messages, the PR link / failure reason text are part
/// of the audited draw list.
fn legibility_dispatch_inflight_task() -> BacklogTask {
    let mut task = legibility_backlog_task();
    task.id = "TASK-6".to_string();
    task.title = "Dispatch in flight".to_string();
    task.labels = vec![DISPATCHING_LABEL.to_string()];
    task.dependencies = vec![];
    task.path = PathBuf::from(format!("{REPO_PATH}/backlog/tasks/task-6.md"));
    task
}

fn legibility_dispatch_dispatched_task() -> BacklogTask {
    let mut task = legibility_backlog_task();
    task.id = "TASK-7".to_string();
    task.title = "Dispatch succeeded".to_string();
    task.labels = vec![DISPATCHED_LABEL.to_string()];
    task.dependencies = vec![];
    task.implementation_notes =
        "Dispatch PR: https://github.com/example/switchbard/pull/42".to_string();
    task.path = PathBuf::from(format!("{REPO_PATH}/backlog/tasks/task-7.md"));
    task
}

fn legibility_dispatch_failed_task() -> BacklogTask {
    let mut task = legibility_backlog_task();
    task.id = "TASK-8".to_string();
    task.title = "Dispatch failed".to_string();
    task.labels = vec![DISPATCH_FAILED_LABEL.to_string()];
    task.dependencies = vec![];
    task.implementation_notes = "Dispatch failed: headless run exited with status 1".to_string();
    task.path = PathBuf::from(format!("{REPO_PATH}/backlog/tasks/task-8.md"));
    task
}

/// A Done, active, CLI-editable task — without one, "Clean Up Old Tasks"
/// stays disabled in every harness below and its text (and the "Archive N
/// Done tasks?" confirm prompt, seeded onto one harness in `views()`) would
/// be WCAG 1.4.3-exempt and silently skip the audit entirely.
fn legibility_done_task() -> BacklogTask {
    let mut task = legibility_backlog_task();
    task.id = "TASK-9".to_string();
    task.title = "Stale done task".to_string();
    task.status = "Done".to_string();
    task.labels = vec![];
    task.dependencies = vec![];
    task.path = PathBuf::from(format!("{REPO_PATH}/backlog/tasks/task-9.md"));
    task
}

fn seed_backlog_project(app: &HiveApp) {
    app.backlog_projects.lock().unwrap().insert(
        PathBuf::from(REPO_PATH),
        BacklogProject {
            root: PathBuf::from(REPO_PATH),
            cli_path: Some(PathBuf::from("/usr/local/bin/backlog")),
            tasks: vec![
                legibility_backlog_task(),
                legibility_blocking_task(),
                legibility_subtask(),
                legibility_dispatch_inflight_task(),
                legibility_dispatch_dispatched_task(),
                legibility_dispatch_failed_task(),
                legibility_done_task(),
            ],
            warnings: vec![],
            loaded_at_unix: 0,
            configured_statuses: vec![],
        },
    );
}

/// Build the harnesses for every view we audit, under `theme`. Covers the top
/// bar + sidebar + each central view, the Agent Context drawer with a file
/// selected (the exact surface in the reported screenshot: small gray paths +
/// preview body), and every task-15/16 Backlog lens with a real, non-empty
/// task selected.
///
/// Scope note: backgrounds are reconstructed from the draw list's filled rects
/// (`background_at`), which covers button fills, selection highlights, and card
/// frames. The remaining blind spot is text painted over an image or gradient
/// (no fill rect to read) — none of the audited views do that today.
fn views(theme: ThemeChoice) -> Vec<(String, Harness<'static, HiveApp>)> {
    write_preview_fixture();
    let suffix = format!(" [{theme:?}]");

    let mut servers_app = seeded_app(); // defaults to ViewTab::Servers
    servers_app.config.ui.theme = theme;
    seed_live_listener(&servers_app);
    let servers = harness(servers_app);

    // TASK-27 (owner-requested UX): the collapsed sidebar rail's own text
    // ("◀", the expand toggle) only ever paints in this state — every other
    // harness here has the default expanded sidebar, which already covers
    // the "▶" collapse toggle.
    // The bulk-selection tint + outline paint only on a selected linked
    // worktree, so it gets its own harness rather than riding on one of the
    // others — see `seed_selected_linked_worktree`.
    let mut selected_app = seeded_app();
    selected_app.config.ui.theme = theme;
    seed_selected_linked_worktree(&mut selected_app);
    let selected_row = harness(selected_app);

    let mut sidebar_collapsed_app = seeded_app();
    sidebar_collapsed_app.config.ui.theme = theme;
    sidebar_collapsed_app.config.ui.sidebar_collapsed = true;
    let sidebar_collapsed = harness(sidebar_collapsed_app);

    let mut agent_app = seeded_app();
    agent_app.config.ui.theme = theme;
    agent_app.view_tab = ViewTab::AgentContext;
    seed_live_listener(&agent_app);
    let agent = harness(agent_app);

    let mut drawer_app = seeded_app();
    drawer_app.config.ui.theme = theme;
    drawer_app.view_tab = ViewTab::AgentContext;
    drawer_app.agent_context_view.selected_id = Some("claude-md".to_string());
    seed_live_listener(&drawer_app);
    let drawer = harness(drawer_app);

    let mut digest_app = seeded_app();
    digest_app.config.ui.theme = theme;
    digest_app.view_tab = ViewTab::Backlog;
    // Digest is `BacklogLens::default()` (task-21), so this fixture leaves
    // `lens` unset deliberately — it's exercising the default a real launch
    // sees, not just picking the enum's first variant.
    seed_backlog_project(&digest_app);
    // "Clean Up Old Tasks" (QA parity matrix LOW gap) is disabled without a
    // Done task and its confirm prompt only ever renders after a click —
    // neither of which the audit's harnesses otherwise trigger. Seeding
    // `cleanup_confirm` directly (same technique the dispatch-state fixtures
    // above use to reach a click-only state) gets its "Archive N Done
    // tasks?" text and Confirm/Cancel buttons into this audit.
    digest_app.backlog_view.cleanup_confirm = true;
    let digest = harness(digest_app);

    let mut list_app = seeded_app();
    list_app.config.ui.theme = theme;
    list_app.view_tab = ViewTab::Backlog;
    // Explicit: `BacklogLens::default()` is `Digest`, not `List`, since
    // task-21 — without this, this fixture would silently audit Digest
    // twice and never touch the List lens's detail pane at all.
    list_app.backlog_view.lens = BacklogLens::List;
    list_app.backlog_view.selected_project = Some(PathBuf::from(REPO_PATH));
    // Expanded so the sub-task tree's nested child row (task-17) is part of
    // what gets audited, not just the collapsed parent's roll-up badge.
    list_app
        .backlog_view
        .expanded_parents
        .insert((PathBuf::from(REPO_PATH), "TASK-1".to_string()));
    seed_backlog_project(&list_app);
    let list = harness(list_app);

    // Two more List-lens harnesses, each explicitly selecting a task the
    // default first-visible-row selection wouldn't reach, so the detail
    // pane's Dispatched (PR link) and Failed (failure reason) messages —
    // each distinct text, not just a differently-colored pill — are part of
    // the audited draw list too.
    let mut dispatched_app = seeded_app();
    dispatched_app.config.ui.theme = theme;
    dispatched_app.view_tab = ViewTab::Backlog;
    dispatched_app.backlog_view.lens = BacklogLens::List;
    dispatched_app.backlog_view.selected_project = Some(PathBuf::from(REPO_PATH));
    dispatched_app.backlog_view.selected_task =
        Some((PathBuf::from(REPO_PATH), "TASK-7".to_string()));
    seed_backlog_project(&dispatched_app);
    let dispatched = harness(dispatched_app);

    let mut dispatch_failed_app = seeded_app();
    dispatch_failed_app.config.ui.theme = theme;
    dispatch_failed_app.view_tab = ViewTab::Backlog;
    dispatch_failed_app.backlog_view.lens = BacklogLens::List;
    dispatch_failed_app.backlog_view.selected_project = Some(PathBuf::from(REPO_PATH));
    dispatch_failed_app.backlog_view.selected_task =
        Some((PathBuf::from(REPO_PATH), "TASK-8".to_string()));
    seed_backlog_project(&dispatch_failed_app);
    let dispatch_failed = harness(dispatch_failed_app);

    // A Done task selected in the detail pane (2026-08-05 fix-wave 2):
    // Backlog.md semantics route a Done task through Complete, not Archive
    // (the real CLI refuses `task archive` on one) — `render_archive` shows
    // a "Complete" button instead, with `archive_confirm` seeded directly
    // (same technique the dispatch states above use) so the "Complete
    // TASK-9?" confirm prompt is audited too, not just the button's resting
    // state.
    let mut done_app = seeded_app();
    done_app.config.ui.theme = theme;
    done_app.view_tab = ViewTab::Backlog;
    done_app.backlog_view.lens = BacklogLens::List;
    done_app.backlog_view.selected_project = Some(PathBuf::from(REPO_PATH));
    done_app.backlog_view.selected_task = Some((PathBuf::from(REPO_PATH), "TASK-9".to_string()));
    done_app.backlog_view.archive_confirm = true;
    // `reconcile_selected_task` (mod.rs) clears a selection outside the
    // currently visible rows, and Done tasks are hidden by default — without
    // this, TASK-9 above would silently fall back to no selection at all,
    // and the "Complete"/confirm text this harness exists to audit would
    // never actually render.
    done_app.backlog_view.show_completed = true;
    seed_backlog_project(&done_app);
    let done = harness(done_app);

    let mut portfolio_app = seeded_app();
    portfolio_app.config.ui.theme = theme;
    portfolio_app.view_tab = ViewTab::Backlog;
    portfolio_app.backlog_view.lens = BacklogLens::Portfolio;
    seed_backlog_project(&portfolio_app);
    let portfolio = harness(portfolio_app);

    let mut board_app = seeded_app();
    board_app.config.ui.theme = theme;
    board_app.view_tab = ViewTab::Backlog;
    board_app.backlog_view.lens = BacklogLens::Board;
    board_app.backlog_view.selected_project = Some(PathBuf::from(REPO_PATH));
    seed_backlog_project(&board_app);
    // TASK-26 (owner-requested UX): bulk-select a task directly on state —
    // the checkbox click itself is UNDRIVABLE-BY-KITTEST for this lens (see
    // backlog_controls.rs's note near board_card_checkbox_reflects_bulk_
    // selection_state) — so the "N selected · Clear" bar's text is only
    // ever painted this way in this audit.
    board_app
        .backlog_view
        .bulk_selected_tasks
        .insert((PathBuf::from(REPO_PATH), "TASK-1".to_string()));
    let board = harness(board_app);

    let mut milestones_app = seeded_app();
    milestones_app.config.ui.theme = theme;
    milestones_app.view_tab = ViewTab::Backlog;
    milestones_app.backlog_view.lens = BacklogLens::Milestones;
    seed_backlog_project(&milestones_app);
    let milestones = harness(milestones_app);

    let mut stats_app = seeded_app();
    stats_app.config.ui.theme = theme;
    stats_app.view_tab = ViewTab::Backlog;
    stats_app.backlog_view.lens = BacklogLens::Statistics;
    seed_backlog_project(&stats_app);
    let stats = harness(stats_app);

    // Owner UX pass (2026-08-05): the onboarding modal's two "success"
    // buttons (`theme::success_button`) were never in any fixture before
    // this pass — the gap that let their white-on-`theme::green()` fill
    // silently fail AA in dark mode (~2.25:1) for who knows how long.
    // `app.onboarding` is pre-seeded to `Ready` (bypassing `render`'s own
    // lazy-init `start_discovery` call) so this harness never kicks off a
    // real filesystem scan of `$HOME` — the same hermeticity concern
    // `app_with_items`'s own doc comment already flags.
    let mut onboarding_empty_app = seeded_app();
    onboarding_empty_app.config.ui.theme = theme;
    onboarding_empty_app.config.ui.onboarding_dismissed = false;
    onboarding_empty_app.config.repos.clear();
    *onboarding_empty_app.onboarding.lock().unwrap() =
        switchbard_gui::ui::onboarding::DiscoveryState::Ready { rows: vec![] };
    let onboarding_empty = harness(onboarding_empty_app);

    let mut onboarding_picker_app = seeded_app();
    onboarding_picker_app.config.ui.theme = theme;
    onboarding_picker_app.config.ui.onboarding_dismissed = false;
    onboarding_picker_app.config.repos.clear();
    *onboarding_picker_app.onboarding.lock().unwrap() =
        switchbard_gui::ui::onboarding::DiscoveryState::Ready {
            rows: vec![switchbard_gui::ui::onboarding::OnboardingRow {
                repo: switchbard_core::discover::DiscoveredRepo {
                    path: PathBuf::from(REPO_PATH),
                    name: REPO_NAME.to_string(),
                    modified: std::time::SystemTime::UNIX_EPOCH,
                },
                selected: true,
            }],
        };
    let onboarding_picker = harness(onboarding_picker_app);

    // Owner UX pass (2026-08-05): the Settings window (repo add/remove,
    // reachable from any view now that "Tracked repos" itself is
    // Servers-only) only ever renders with `settings_open` set — none of
    // the fixtures above touch it, so this is its only audit coverage.
    let mut settings_app = seeded_app();
    settings_app.config.ui.theme = theme;
    settings_app.settings_open = true;
    let settings = harness(settings_app);

    vec![
        (
            format!("Servers view (top bar + sidebar + workspace){suffix}"),
            servers,
        ),
        (
            format!("Servers view (sidebar collapsed){suffix}"),
            sidebar_collapsed,
        ),
        (
            format!("Onboarding · no repos found{suffix}"),
            onboarding_empty,
        ),
        (
            format!("Onboarding · repo picker (one selected){suffix}"),
            onboarding_picker,
        ),
        (
            format!("Workspace · bulk-selected worktree row{suffix}"),
            selected_row,
        ),
        (format!("Agent Context view{suffix}"), agent),
        (
            format!("Agent Context · file selected (path + preview){suffix}"),
            drawer,
        ),
        (format!("Backlog · Digest lens (default){suffix}"), digest),
        (format!("Backlog · List lens (task detail){suffix}"), list),
        (
            format!("Backlog · List lens (dispatched task detail){suffix}"),
            dispatched,
        ),
        (
            format!("Backlog · List lens (dispatch-failed task detail){suffix}"),
            dispatch_failed,
        ),
        (
            format!("Backlog · List lens (Done task, Complete confirm){suffix}"),
            done,
        ),
        (format!("Backlog · Board lens{suffix}"), board),
        (format!("Backlog · Milestones lens{suffix}"), milestones),
        (format!("Backlog · Portfolio lens{suffix}"), portfolio),
        (format!("Backlog · Statistics lens{suffix}"), stats),
        (format!("Settings window{suffix}"), settings),
    ]
}

#[test]
fn ui_text_meets_legibility_contract() {
    let mut violations = Vec::new();
    for theme in [ThemeChoice::Light, ThemeChoice::Dark] {
        for (name, mut harness) in views(theme) {
            // Let scroll areas / layout settle before reading the draw list.
            harness.run();
            violations.extend(audit_owned(name, &harness));
        }
    }

    assert!(violations.is_empty(), "{}", report(&violations));
}

/// Sanity: the contract math itself behaves (so a failure above is about the
/// UI, not the metric). Anchored on values from `theme.rs`'s own comments.
#[test]
fn contract_math_is_sound() {
    let white = Color32::WHITE;
    let black = Color32::BLACK;
    assert!((legibility::contrast_ratio(white, black) - 21.0).abs() < 0.1);
    assert!((legibility::contrast_ratio(white, white) - 1.0).abs() < 0.001);

    // WEAK_TEXT (#4A4A4A) on the #F8F8F8 panel — theme.rs claims ~8.4:1.
    let weak_text = Color32::from_rgb(0x4A, 0x4A, 0x4A);
    let panel = Color32::from_gray(248);
    let ratio = legibility::contrast_ratio(weak_text, panel);
    assert!(
        (8.0..9.0).contains(&ratio),
        "expected ~8.4:1, got {ratio:.2}"
    );

    // A translucent foreground composited over its background must match the
    // equivalent opaque blend.
    let half = Color32::from_rgba_unmultiplied(0, 0, 0, 128);
    let flat = legibility::composite_over(half, panel);
    assert!(
        flat.r() > 100 && flat.r() < 160,
        "composited gray: {flat:?}"
    );
}
