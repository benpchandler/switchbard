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

mod common;

use std::collections::BTreeMap;
use std::fmt::Write as _;

use common::{harness, seeded_app, REPO_NAME, REPO_PATH};
use eframe::egui::{epaint::Shape, Color32, Pos2, Rect};
use egui_kittest::Harness;
use switchbard_core::{AttributedListener, LocalListener};
use switchbard_gui::app::HiveApp;
use switchbard_gui::runtime::ViewTab;
use switchbard_gui::ui::legibility;

/// One painted run of text with a single resolved size + color.
struct TextRun {
    text: String,
    size: f32,
    /// Premultiplied color as painted (before compositing over the background).
    color: Color32,
    pos: Pos2,
}

/// A run that failed the contract, with the perceived contrast and why.
struct Violation {
    view: &'static str,
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
}

/// Recursively pull `Shape::Text` runs and `Shape::Rect` fills out of the draw
/// list. egui sometimes nests shapes inside `Shape::Vec`, so we descend too.
fn collect(shape: &Shape, runs: &mut Vec<TextRun>, rects: &mut Vec<FilledRect>) {
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
                    .get(section.byte_range.clone())
                    .unwrap_or_default();
                if text.trim().is_empty() {
                    continue;
                }
                runs.push(TextRun {
                    text: text.to_string(),
                    size: section.format.font_id.size,
                    color,
                    pos: t.pos,
                });
            }
        }
        Shape::Rect(r) if r.fill.a() > 0 => {
            rects.push(FilledRect {
                rect: r.rect,
                fill: r.fill,
            });
        }
        Shape::Vec(shapes) => {
            for s in shapes {
                collect(s, runs, rects);
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

/// Audit one rendered view against the legibility contract.
fn audit(view: &'static str, harness: &Harness<'_, HiveApp>) -> Vec<Violation> {
    let panel = harness.ctx.style().visuals.panel_fill;

    let mut runs = Vec::new();
    let mut rects = Vec::new();
    for clipped in &harness.output().shapes {
        collect(&clipped.shape, &mut runs, &mut rects);
    }

    let mut violations = Vec::new();
    for run in runs {
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
                view,
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
        by_view.entry(v.view).or_default().push(v);
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

/// Build the harnesses for every view we audit. Covers the top bar + sidebar +
/// each central view, plus the Agent Context drawer with a file selected (the
/// exact surface in the reported screenshot: small gray paths + preview body).
///
/// Scope note: backgrounds are reconstructed from the draw list's filled rects
/// (`background_at`), which covers button fills, selection highlights, and card
/// frames. The remaining blind spot is text painted over an image or gradient
/// (no fill rect to read) — none of the audited views do that today.
fn views() -> Vec<(&'static str, Harness<'static, HiveApp>)> {
    write_preview_fixture();

    let servers_app = seeded_app(); // defaults to ViewTab::Servers
    seed_live_listener(&servers_app);
    let servers = harness(servers_app);

    let mut agent_app = seeded_app();
    agent_app.view_tab = ViewTab::AgentContext;
    seed_live_listener(&agent_app);
    let agent = harness(agent_app);

    let mut drawer_app = seeded_app();
    drawer_app.view_tab = ViewTab::AgentContext;
    drawer_app.agent_context_view.selected_id = Some("claude-md".to_string());
    seed_live_listener(&drawer_app);
    let drawer = harness(drawer_app);

    vec![
        ("Servers view (top bar + sidebar + workspace)", servers),
        ("Agent Context view", agent),
        ("Agent Context · file selected (path + preview)", drawer),
    ]
}

#[test]
fn ui_text_meets_legibility_contract() {
    let mut violations = Vec::new();
    for (name, mut harness) in views() {
        // Let scroll areas / layout settle before reading the draw list.
        harness.run();
        violations.extend(audit(name, &harness));
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
