//! Contrast gates over rendered sRGB cells, not inferred terminal colors.
//! APCA-W3 0.0.98G-4g math: https://apcaw3.myndex.com/docs/APCA-W3-LaTeX.html
//! These are project design thresholds, not accessibility compliance claims.
mod harness;

use crossterm::event::KeyCode;
use harness::*;
use ratatui::style::{Color, Modifier};
use switchbard_core::{claim_work, WorkIdentity};

fn command(h: &mut Harness, text: &str) {
    h.type_text(&format!(":{text}"));
    h.press(KeyCode::Enter);
}

fn luminance(color: Color) -> f64 {
    let Color::Rgb(r, g, b) = color else {
        panic!("contrast requires declared sRGB, got {color:?}")
    };
    let value: f64 = [(r, 0.2126729), (g, 0.7151522), (b, 0.0721750)]
        .into_iter()
        .map(|(c, weight)| (f64::from(c) / 255.0).powf(2.4) * weight)
        .sum();
    if value < 0.022 {
        value + (0.022 - value).powf(1.414)
    } else {
        value
    }
}

fn contrast(foreground: Color, background: Color) -> f64 {
    let text = luminance(foreground);
    let canvas = luminance(background);
    if (text - canvas).abs() < 0.0005 {
        return 0.0;
    }
    let raw = if canvas > text {
        (canvas.powf(0.56) - text.powf(0.57)) * 1.14
    } else {
        (canvas.powf(0.65) - text.powf(0.62)) * 1.14
    };
    if raw.abs() < 0.1 {
        0.0
    } else {
        (raw - raw.signum() * 0.027) * 100.0
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
    let value = contrast(foreground, background).abs();
    assert!((minimum..=100.0).contains(&value), "{context}: {needle}, {foreground:?}/{background:?} = Lc {value:.2}, expected {minimum}..100");
}

#[test]
fn every_colored_preset_keeps_body_secondary_and_roles_readable_on_rendered_bands() {
    for name in ["berg", "bloomberg", "darkroom", "light"] {
        let mut h = Harness::new();
        command(&mut h, &format!("theme {name}"));
        gate(&h, "Fix login", 75.0, &format!("{name} selected"));
        gate(&h, "Write onboarding guide", 75.0, &format!("{name} body"));
        gate(&h, "4 title", 60.0, &format!("{name} header"));
        command(&mut h, "group status");
        gate(&h, "▸ To Do", 60.0, &format!("{name} heading"));
        command(&mut h, "group off");
        for (role, floor) in [
            ("quiet", 45.0),
            ("strong", 75.0),
            ("alert", 60.0),
            ("band", 75.0),
            ("struck", 75.0),
        ] {
            command(&mut h, &format!("paint column:title={role}"));
            gate(&h, "Fix login", floor, &format!("{name} {role} selected"));
            gate(
                &h,
                "Write onboarding guide",
                floor,
                &format!("{name} {role}"),
            );
        }
    }
}

#[test]
fn working_cycle_preserves_readability_and_disable_keeps_a_steady_band() {
    for name in ["berg", "bloomberg", "darkroom", "light"] {
        let mut h = Harness::new();
        std::fs::write(
            &h.config_path,
            format!("return {{ theme = '{name}', work = {{ period_ms = 40, frames = 40 }} }}"),
        )
        .unwrap();
        h.app.tick();
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
    for name in ["berg", "bloomberg", "darkroom", "light"] {
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
        let actual = contrast(
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
    for name in ["berg", "bloomberg", "darkroom", "light"] {
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
