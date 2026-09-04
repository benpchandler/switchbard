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
        Some(ratatui::style::Color::Rgb(0x2a, 0x6b, 0x5a)),
        "the row wears the berg working band at full glow"
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

#[test]
fn the_band_pulses_through_brightness_levels_and_help_lists_pass() {
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
    assert!(
        h.app.next_blink().is_some(),
        "a working row schedules a redraw"
    );
    let mut seen = std::collections::HashSet::new();
    for _ in 0..200 {
        h.render();
        seen.insert(cell_bg(&h, &title));
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    assert!(
        seen.len() >= 4,
        "the band fades through several levels: {seen:?}"
    );
    let help = h.press(KeyCode::Char('?'));
    assert!(help.contains("pass"), "{help}");
}
