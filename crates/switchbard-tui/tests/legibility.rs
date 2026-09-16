//! Contrast gates over rendered sRGB cells, not inferred terminal colors.
//! The APCA-W3 0.0.98G-4g math is the shipped one (`switchbard_tui::legibility`),
//! so these gates and the theme's own fill refusal answer from one implementation.
//! These are project design thresholds, not accessibility compliance claims.
mod harness;

use crossterm::event::KeyCode;
use harness::*;
use ratatui::style::{Color, Modifier};
use switchbard_core::{claim_work, WorkIdentity};
use switchbard_tui::legibility::contrast;

/// The ink a preset owns, and the Lc each one owes on any fill it lands on.
/// Palette tokens are deliberately absent: a palette is chosen independently of
/// the preset (`:palette light` pairs one with the light canvas on purpose), so
/// categorical hues carry no preset contrast claim, on a fill or off it.
const INK_FLOORS: [(&str, f64); 4] = [
    ("quiet", 45.0),
    ("strong", 75.0),
    ("alert", 60.0),
    ("struck", 75.0),
];

/// Every fill the presets can put under that ink: the neutral band, each
/// declared highlight slot, and one slot no preset declares, which exercises
/// the fill derived from the palette at a fixed step off the canvas.
const FILLS: [&str; 5] = ["band", "h1", "h2", "h3", "h7"];

const COLORED_PRESETS: [&str; 4] = ["berg", "bloomberg", "darkroom", "light"];

fn command(h: &mut Harness, text: &str) {
    h.type_text(&format!(":{text}"));
    h.press(KeyCode::Enter);
}

fn measure(foreground: Color, background: Color) -> f64 {
    contrast(foreground, background).unwrap_or_else(|| {
        panic!("contrast requires declared sRGB, got {foreground:?} on {background:?}")
    })
}

/// The pulse at its trough and at its peak over a cell as it actually rendered.
/// `App::work_glow` is a function of elapsed wall-clock time, so the endpoints
/// are reached through the two shipped functions the row renderer composes
/// (`working_style` for the band, `working_fg` for the lift) rather than by
/// sampling frames until they happen to appear. Call it before the row is
/// claimed, so the ink it starts from is the painted ink and not an
/// already-lifted frame of the cycle.
fn gate_pulse_over(h: &Harness, needle: &str, minimum: f64, context: &str) {
    let rest_fg = cell_fg(h, needle).unwrap_or_else(|| panic!("missing {needle} in {context}"));
    let rest_bg = cell_bg(h, needle).unwrap();
    let theme = &h.app.config.theme;
    for glow in [0.0, 1.0] {
        let background = theme.working_style(glow).bg.unwrap_or(rest_bg);
        let foreground = theme.working_fg(Some(rest_fg), glow);
        let value = measure(foreground, background).abs();
        assert!(
            (minimum..=100.0).contains(&value),
            "{context} at glow {glow}: {foreground:?}/{background:?} = Lc {value:.2}, expected {minimum}..100"
        );
    }
}

fn gate(h: &Harness, needle: &str, minimum: f64, context: &str) {
    let foreground = cell_fg(h, needle).unwrap_or_else(|| {
        panic!(
            "missing {needle} in {context}\n{}",
            switchbard_tui::view::buffer_text(h.terminal.backend().buffer())
        )
    });
    let background = cell_bg(h, needle).unwrap();
    if minimum == 60.0 {
        let buffer = h.terminal.backend().buffer();
        let style = buffer
            .content
            .chunks(usize::from(buffer.area.width))
            .find_map(|row| {
                let text: String = row.iter().map(|cell| cell.symbol()).collect();
                text.find(needle)
                    .map(|index| row[text[..index].chars().count()].modifier)
            })
            .expect("the contrast subject is rendered");
        assert!(
            style.contains(Modifier::BOLD),
            "{context}: the Lc60 threshold requires bold text"
        );
    }
    let value = measure(foreground, background).abs();
    assert!((minimum..=100.0).contains(&value), "{context}: {needle}, {foreground:?}/{background:?} = Lc {value:.2}, expected {minimum}..100");
}

#[test]
fn every_colored_preset_keeps_body_secondary_and_roles_readable_on_rendered_bands() {
    for name in COLORED_PRESETS {
        let mut h = Harness::new();
        command(&mut h, &format!("theme {name}"));
        gate(&h, "Fix login", 75.0, &format!("{name} selected"));
        gate(&h, "Write onboarding guide", 75.0, &format!("{name} body"));
        // TASK-234 split the header cell into a "4" key span (Surface::Keys)
        // and a "title" label span (Surface::Header, still bold); gate the
        // label's own ink/background pair rather than the leading digit's.
        gate(&h, "title", 60.0, &format!("{name} header"));
        command(&mut h, "group status");
        gate(&h, "▸ To Do", 60.0, &format!("{name} heading"));
        command(&mut h, "group off");
        for (role, floor) in INK_FLOORS {
            command(&mut h, &format!("paint column:title={role}"));
            gate(&h, "Fix login", floor, &format!("{name} {role} selected"));
            gate(
                &h,
                "Write onboarding guide",
                floor,
                &format!("{name} {role}"),
            );
        }
        // Every composed ink on every composed fill, plus each fill's own
        // default ink, and the selected row's patch over the same cell.
        for fill in FILLS {
            command(&mut h, &format!("paint column:title={fill}"));
            gate(
                &h,
                "Write onboarding guide",
                75.0,
                &format!("{name} {fill}"),
            );
            gate(&h, "Fix login", 75.0, &format!("{name} {fill} selected"));
            for (ink, floor) in INK_FLOORS {
                command(&mut h, &format!("paint column:title={fill}+{ink}"));
                gate(
                    &h,
                    "Write onboarding guide",
                    floor,
                    &format!("{name} {fill}+{ink}"),
                );
                gate(
                    &h,
                    "Fix login",
                    floor,
                    &format!("{name} {fill}+{ink} selected"),
                );
            }
        }
    }
}

#[test]
fn working_cycle_preserves_readability_and_disable_keeps_a_steady_band() {
    for name in COLORED_PRESETS {
        let mut h = Harness::new();
        std::fs::write(
            &h.config_path,
            format!("return {{ theme = '{name}', work = {{ period_ms = 40, frames = 40 }} }}"),
        )
        .unwrap();
        h.app.tick();
        // The working band replaces a painted fill, so the ink a fill rule
        // chose has to survive the whole pulse over that cell too.
        command(&mut h, "paint column:title=h1+alert");
        gate_pulse_over(
            &h,
            "Fix login",
            60.0,
            &format!("{name} working over h1+alert"),
        );
        command(&mut h, "paint off");
        let id = h.app.selected_task().unwrap().id.clone();
        claim_work(
            &h.root.join("work"),
            &WorkIdentity {
                session_id: format!("contrast-{name}"),
                pid: std::process::id(),
                agent: "codex".into(),
            },
            &h.root,
            &id,
        )
        .unwrap();
        h.app.tick();
        let mut backgrounds = std::collections::HashSet::new();
        for _ in 0..100 {
            h.render();
            gate(&h, "Fix login", 75.0, &format!("{name} working"));
            backgrounds.insert(cell_bg(&h, "Fix login"));
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        assert!(
            backgrounds.len() > 2,
            "motion endpoints rendered for {name}"
        );
        std::fs::write(
            &h.config_path,
            format!("return {{ theme = '{name}', work = {{ period_ms = 0 }} }}"),
        )
        .unwrap();
        h.app.tick();
        h.render();
        let steady = cell_bg(&h, "Fix login");
        std::thread::sleep(std::time::Duration::from_millis(25));
        h.render();
        assert_eq!(steady, cell_bg(&h, "Fix login"));
        assert!(h.app.next_blink().is_none());
        gate(&h, "Fix login", 75.0, &format!("{name} steady"));
    }
}

#[test]
fn details_inherit_declared_body_ink_on_every_colored_canvas() {
    for name in COLORED_PRESETS {
        let mut h = Harness::new();
        command(&mut h, &format!("theme {name}"));
        h.press(KeyCode::Enter);
        gate(&h, "Description of", 75.0, &format!("{name} detail body"));
    }
}

// Published keystone pairs, including both polarities and the near-black clamp:
// https://github.com/Myndex/apca-w3/blob/master/test/test.html
#[test]
fn rendered_reference_pairs_match_published_apca_values() {
    for (foreground, background, expected) in [
        ("#888888", "#ffffff", 63.056469930209424),
        ("#ffffff", "#888888", -68.54146436644962),
        ("#000000", "#aaaaaa", 58.146262578561334),
        ("#112233", "#ddeeff", 91.66830811481631),
    ] {
        let mut h = Harness::new();
        std::fs::write(&h.config_path, format!(
            "return {{ theme = {{ background = '{background}', text = {{ fg = '{foreground}' }} }} }}"
        )).unwrap();
        h.app.tick();
        h.render();
        let actual = measure(
            cell_fg(&h, "Write onboarding guide").unwrap(),
            cell_bg(&h, "Write onboarding guide").unwrap(),
        );
        assert!(
            (actual - expected).abs() < 0.00000001,
            "{foreground}/{background}: {actual} != {expected}"
        );
    }
}

#[test]
fn navigation_context_identity_links_and_hints_stay_readable() {
    for name in COLORED_PRESETS {
        let mut h = Harness::new();
        seed_in_project(&h.root, "Ship Atlas", "To Do", "Atlas", None);
        std::fs::write(
            h.root.join("views.lua"),
            "return { [1] = { columns = 'id,project,title' } }",
        )
        .unwrap();
        h.app = open_app(&h.root, &h.config_path);
        command(&mut h, &format!("theme {name}"));
        gate(&h, "[Tasks]", 60.0, &format!("{name} active navigation"));
        gate(&h, "Inbox", 45.0, &format!("{name} navigation title"));
        gate(&h, "switch page", 45.0, &format!("{name} keys"));
        gate(&h, "Atlas", 45.0, &format!("{name} project link"));
        gate(
            &h,
            "1              Fix login",
            45.0,
            &format!("{name} identity label"),
        );
        let repo = h.root.file_name().unwrap().to_str().unwrap();
        gate(&h, repo, 60.0, &format!("{name} repository title"));
        h.press(KeyCode::Esc);
        gate(&h, "v views", 45.0, &format!("{name} footer hint"));
        h.type_text("/login");
        h.press(KeyCode::Enter);
        gate(&h, "/ login", 45.0, &format!("{name} filter context"));
    }
}

/// The owner's example (TASK-237): a pink fill with red ink for alert, on the
/// preset they actually use, readable at rest, under the cursor, and through a
/// working row's whole pulse.
#[test]
fn a_pink_fill_with_alert_ink_reads_at_rest_selected_and_while_working() {
    let mut h = Harness::new();
    std::fs::write(
        &h.config_path,
        "return { theme = 'berg', work = { period_ms = 40, frames = 40 } }",
    )
    .unwrap();
    h.app.tick();
    command(&mut h, "paint column:title=h3+alert");
    let fill = h
        .app
        .config
        .theme
        .highlight_style(3, &h.app.config.palette)
        .and_then(|style| style.bg);
    assert_eq!(cell_bg(&h, "Write onboarding guide"), fill, "the pink fill");
    gate(&h, "Write onboarding guide", 60.0, "berg pink alert");
    gate(&h, "Fix login", 60.0, "berg pink alert selected");
    gate_pulse_over(&h, "Fix login", 60.0, "berg pink alert working");
    let id = h.app.selected_task().unwrap().id.clone();
    claim_work(
        &h.root.join("work"),
        &WorkIdentity {
            session_id: "contrast-pink".into(),
            pid: std::process::id(),
            agent: "codex".into(),
        },
        &h.root,
        &id,
    )
    .unwrap();
    h.app.tick();
    h.render();
    assert_ne!(
        cell_bg(&h, "Fix login"),
        fill,
        "a claimed row wears the working band over the fill"
    );
    gate(&h, "Fix login", 60.0, "berg pink alert working frame");
    std::fs::write(
        &h.config_path,
        "return { theme = 'berg', work = { period_ms = 0 } }",
    )
    .unwrap();
    h.app.tick();
    h.render();
    gate(&h, "Fix login", 60.0, "berg pink alert working peak");
}

/// A fill a user supplies is their own choice, outside the preset guarantee.
/// This one still has to read where it is painted.
#[test]
fn a_user_supplied_light_fill_reads_with_the_ink_declared_beside_it() {
    let mut h = Harness::new();
    std::fs::write(
        &h.config_path,
        r##"return { theme = { emphasis = { alert = { fg = "#8a1c24", bg = "#f6c8d4" } } } }"##,
    )
    .unwrap();
    h.app.tick();
    command(&mut h, "paint column:title=alert");
    assert_eq!(
        cell_bg(&h, "Write onboarding guide"),
        Some(Color::Rgb(0xf6, 0xc8, 0xd4)),
        "the declared fill survives load"
    );
    gate(&h, "Write onboarding guide", 60.0, "user pink alert");
}
