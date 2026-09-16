//! Live work: rows a running agent session has claimed light up, and `w`
//! passes them (TASK-150). Sessions are seeded through the core store the
//! way `sb work claim` writes it, with this test process as the "agent" so
//! the claim is live, or an impossible pid so it is dead.

mod harness;

use crossterm::event::KeyCode;
use harness::*;
use switchbard_core::{claim_work, WorkIdentity};

fn session(id: &str, pid: u32) -> WorkIdentity {
    WorkIdentity {
        session_id: id.to_string(),
        pid,
        agent: "claude".to_string(),
    }
}

fn steady_blink(h: &mut Harness) {
    std::fs::write(&h.config_path, "return { work = { period_ms = 0 } }").unwrap();
    h.app.tick();
}

fn show_work_column(h: &mut Harness) {
    h.press(KeyCode::Char('c'));
    h.type_text("w");
    h.press(KeyCode::Esc);
}

#[test]
fn a_live_session_lights_its_row_and_a_dead_one_is_forgotten() {
    let mut h = Harness::new();
    steady_blink(&mut h);
    let id = h.app.selected_task().unwrap().id.clone();
    let title = h.selected_title();
    let work = h.root.join("work");
    claim_work(
        &work,
        &session("live-1234-abcd", std::process::id()),
        &h.root,
        &id,
    )
    .unwrap();
    claim_work(
        &work,
        &session("dead-1234-abcd", u32::MAX - 1),
        &h.root,
        &id,
    )
    .unwrap();
    h.app.tick();
    show_work_column(&mut h);
    let screen = h.render();
    assert!(screen.contains("working:1"), "{screen}");
    assert!(
        header_line(&screen).contains("5 work"),
        "{}",
        header_line(&screen)
    );
    let row = screen
        .lines()
        .find(|line| line.contains(title.as_str()))
        .unwrap()
        .to_string();
    assert!(
        row.contains('●'),
        "one glyph for the one live session: {row}"
    );
    assert!(
        !row.contains("●●"),
        "the dead session does not count: {row}"
    );
    assert_eq!(
        cell_bg(&h, &title),
        Some(ratatui::style::Color::Rgb(0x16, 0x3b, 0x30)),
        "the row wears the berg working band at full glow"
    );
    let rest = cell_fg(&h, "Write onboarding guide").unwrap();
    let lit = cell_fg(&h, &title).unwrap();
    let brightness = |color: ratatui::style::Color| match color {
        ratatui::style::Color::Rgb(r, g, b) => u32::from(r) + u32::from(g) + u32::from(b),
        _ => panic!("expected an rgb colour, got {color:?}"),
    };
    assert!(
        brightness(lit) > brightness(rest),
        "text on the lit row is brighter than at rest: {lit:?} vs {rest:?}"
    );
    assert!(
        !work.join("dead-1234-abcd.json").exists(),
        "a dead session's record is pruned on read"
    );
    let other = screen
        .lines()
        .find(|line| line.contains("Write onboarding guide"))
        .unwrap();
    assert!(!other.contains('●'), "{other}");
}

#[test]
fn several_sessions_on_one_task_show_one_glyph_each_and_the_detail_names_them() {
    let mut h = Harness::new();
    steady_blink(&mut h);
    let id = h.app.selected_task().unwrap().id.clone();
    let title = h.selected_title();
    let work = h.root.join("work");
    claim_work(
        &work,
        &session("aaaa1111-1", std::process::id()),
        &h.root,
        &id,
    )
    .unwrap();
    claim_work(
        &work,
        &session("bbbb2222-2", std::process::id()),
        &h.root,
        &id,
    )
    .unwrap();
    h.app.tick();
    show_work_column(&mut h);
    let screen = h.render();
    assert!(screen.contains("working:2"), "{screen}");
    let row = screen
        .lines()
        .find(|line| line.contains(title.as_str()))
        .unwrap();
    assert!(row.contains("●●"), "{row}");
    let detail = h.press(KeyCode::Enter);
    assert!(detail.contains("working · claude aaaa1111"), "{detail}");
    assert!(detail.contains("working · claude bbbb2222"), "{detail}");
    assert!(detail.contains("(pid "), "{detail}");
}

#[test]
fn w_passes_the_task_ending_every_claim_and_dropping_the_ball() {
    let mut h = Harness::new();
    steady_blink(&mut h);
    let id = h.app.selected_task().unwrap().id.clone();
    let title = h.selected_title();
    let work = h.root.join("work");
    claim_work(
        &work,
        &session("aaaa1111-1", std::process::id()),
        &h.root,
        &id,
    )
    .unwrap();
    claim_work(
        &work,
        &session("bbbb2222-2", std::process::id()),
        &h.root,
        &id,
    )
    .unwrap();
    switchbard_core::set_backlog_ball(&h.root, &id, Some(switchbard_core::Ball::Agent)).unwrap();
    h.app.tick();
    show_work_column(&mut h);
    let screen = h.press(KeyCode::Char('w'));
    assert!(
        screen.contains(&format!("{id}: passed · released from aaaa1111, bbbb2222")),
        "{screen}"
    );
    assert!(!screen.contains("working:"), "{screen}");
    let row = screen
        .lines()
        .find(|line| line.contains(title.as_str()))
        .unwrap();
    assert!(!row.contains('●'), "{row}");
    assert!(
        !h.app
            .selected_task()
            .unwrap()
            .labels
            .iter()
            .any(|label| label.starts_with("ball:")),
        "the pass drops the ball"
    );
    let screen = h.press(KeyCode::Char('w'));
    assert!(screen.contains("no session is working it"), "{screen}");
}

/// TASK-155: the pulse used to push the row's text toward black in the
/// trough, and a darkened amber is brown. The band carries the dark half; the
/// text only ever brightens, so no frame of the pulse is muddier than a row
/// sitting at rest.
#[test]
fn the_pulse_never_darkens_a_working_rows_text_below_its_rest_colour() {
    let mut h = Harness::new();
    std::fs::write(
        &h.config_path,
        "return { work = { period_ms = 40, frames = 40 } }",
    )
    .unwrap();
    h.app.tick();
    let id = h.app.selected_task().unwrap().id.clone();
    let title = h.selected_title();
    claim_work(
        &h.root.join("work"),
        &session("aaaa1111-1", std::process::id()),
        &h.root,
        &id,
    )
    .unwrap();
    h.app.tick();
    let channels = |color: ratatui::style::Color| match color {
        ratatui::style::Color::Rgb(r, g, b) => (u32::from(r), u32::from(g), u32::from(b)),
        other => panic!("expected an rgb colour, got {other:?}"),
    };
    let rest = channels(cell_fg(&h, "Write onboarding guide").unwrap());
    let mut seen = std::collections::HashSet::new();
    for _ in 0..200 {
        h.render();
        let lit = channels(cell_fg(&h, &title).unwrap());
        assert!(
            lit.0 >= rest.0 && lit.1 >= rest.1 && lit.2 >= rest.2,
            "no frame of the pulse is dimmer than rest: {lit:?} vs {rest:?}"
        );
        seen.insert(lit);
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    assert!(
        seen.len() >= 4,
        "the text still breathes through several levels: {seen:?}"
    );
    assert!(
        seen.contains(&rest),
        "the trough is the rest colour itself: {seen:?}"
    );
}

/// OKLab lightness of a declared sRGB color (Björn Ottosson's OKLab, the
/// same construction as `src/oklch.rs`, kept independent here the way
/// `tests/legibility.rs` keeps its own APCA luminance/contrast: two
/// implementations that must agree catch more than one that just gets
/// trusted).
fn oklab_lightness(color: ratatui::style::Color) -> f64 {
    let ratatui::style::Color::Rgb(r, g, b) = color else {
        panic!("expected an rgb colour, got {color:?}")
    };
    let linear = |channel: u8| {
        let c = f64::from(channel) / 255.0;
        if c <= 0.040_45 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    };
    let (r, g, b) = (linear(r), linear(g), linear(b));
    let l = 0.412_221_470_8 * r + 0.536_332_536_3 * g + 0.051_445_992_9 * b;
    let m = 0.211_903_498_2 * r + 0.680_699_545_1 * g + 0.107_396_956_6 * b;
    let s = 0.088_302_461_9 * r + 0.281_718_837_6 * g + 0.629_978_700_5 * b;
    0.210_454_255_3 * l.cbrt() + 0.793_617_785_0 * m.cbrt() - 0.004_072_046_8 * s.cbrt()
}

/// APCA Lc between declared sRGB colors, independently of `tests/legibility.rs`'s
/// copy: this file only needs it to prove the trough itself clears the
/// working-row floor, not to re-audit every surface.
fn apca_lc(foreground: ratatui::style::Color, background: ratatui::style::Color) -> f64 {
    let luminance = |color: ratatui::style::Color| {
        let ratatui::style::Color::Rgb(r, g, b) = color else {
            panic!("expected an rgb colour, got {color:?}")
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
    };
    let (text, canvas) = (luminance(foreground), luminance(background));
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

/// TASK-241: raw sRGB luminance compresses a small perceptual swing into a
/// larger-looking channel-value change (and vice versa), which is exactly
/// how the old scale-based pulse read as a 20% luminance swing while still
/// being reported "barely perceptible" on darkroom. Sampling OKLab
/// lightness instead is the one metric this test trusts for the swing; the
/// trough's own APCA Lc proves the TASK-218 never-full-off floor holds at
/// the darkest (or, on `light`, the least-saturated) point of the cycle.
#[test]
fn the_band_pulses_through_an_oklab_lightness_swing_on_every_preset() {
    for name in ["berg", "bloomberg", "darkroom", "light"] {
        let mut h = Harness::new();
        // A slower period than the other pulse tests: this one measures the
        // true min/max lightness by dense sampling rather than checking a
        // handful of distinct levels, so it needs fine phase resolution
        // across one full cycle rather than many coarsely-sampled ones.
        std::fs::write(
            &h.config_path,
            format!("return {{ theme = '{name}', work = {{ period_ms = 200, frames = 40 }} }}"),
        )
        .unwrap();
        h.app.tick();
        let id = h.app.selected_task().unwrap().id.clone();
        let title = h.selected_title();
        claim_work(
            &h.root.join("work"),
            &session("aaaa1111-1", std::process::id()),
            &h.root,
            &id,
        )
        .unwrap();
        h.app.tick();
        assert!(
            h.app.next_blink().is_some(),
            "{name}: a working row schedules a redraw"
        );
        let mut seen = std::collections::HashSet::new();
        let mut frames = Vec::new();
        for _ in 0..300 {
            h.render();
            let bg = cell_bg(&h, &title).unwrap_or_else(|| panic!("{name}: band renders"));
            let fg = cell_fg(&h, &title).unwrap_or_else(|| panic!("{name}: band renders"));
            seen.insert(bg);
            frames.push((oklab_lightness(bg), bg, fg));
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        assert!(
            seen.len() >= 4,
            "{name}: the band fades through several levels: {seen:?}"
        );
        let canvas = h.app.config.theme.background().unwrap();
        assert!(
            seen.iter().all(|color| *color != canvas),
            "{name}: every frame must retain the working band: {seen:?}"
        );
        let (trough_l, trough_bg, trough_fg) = *frames
            .iter()
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .expect("200 frames were sampled");
        let (peak_l, ..) = *frames
            .iter()
            .max_by(|a, b| a.0.total_cmp(&b.0))
            .expect("200 frames were sampled");
        let swing = peak_l - trough_l;
        assert!(
            (0.14..=0.26).contains(&swing),
            "{name}: OKLab lightness swing {swing:.3} (trough {trough_l:.3}, peak {peak_l:.3})"
        );
        let floor = apca_lc(trough_fg, trough_bg).abs();
        assert!(
            floor >= 75.0,
            "{name}: trough Lc {floor:.2} below the working-row floor ({trough_fg:?}/{trough_bg:?})"
        );
    }
    let mut h = Harness::new();
    let help = h.press(KeyCode::Char('?'));
    assert!(help.contains("pass"), "{help}");
}
