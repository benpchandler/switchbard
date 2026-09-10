//! Real-key date painting over disk-backed tasks and optional real GitHub history.
mod harness;
use chrono::{Duration, Utc};
use crossterm::event::KeyCode;
use harness::*;
use ratatui::{backend::TestBackend, style::Color, Terminal};

fn pick(h: &mut Harness, label: &str) {
    let picker = h.app.picker.as_ref().expect("picker open");
    let options = picker.matching();
    let index = options
        .iter()
        .position(|o| o.label == label || o.label.starts_with(&format!("{label} ·")))
        .unwrap_or_else(|| {
            panic!(
                "missing {label}: {:?}",
                options.iter().map(|o| &o.label).collect::<Vec<_>>()
            )
        });
    let selected = picker.selected;
    for _ in 0..selected {
        h.press(KeyCode::Up);
    }
    for _ in 0..index {
        h.press(KeyCode::Down);
    }
    h.press(KeyCode::Enter);
}

fn dated() -> Harness {
    let mut h = Harness::new();
    for entry in std::fs::read_dir(h.root.join("backlog/tasks")).expect("fixture directory") {
        std::fs::remove_file(entry.expect("entry").path()).expect("remove fixture");
    }
    let today = Utc::now().date_naive();
    for (index, age) in [-1, 0, 1, 6, 7, 29, 30].into_iter().enumerate() {
        let date = today - Duration::days(age);
        let text = format!("---\nid: TASK-{}\ntitle: Age {age} boundary\nstatus: To Do\ncreated_date: '{date} 23:59'\n---\n", index + 1);
        std::fs::write(
            h.root.join(format!("backlog/tasks/task-{}.md", index + 1)),
            text,
        )
        .expect("dated task");
    }
    for (index, value) in [(8, "2026-02-30 12:00"), (9, ""), (10, "not a date")] {
        std::fs::write(h.root.join(format!("backlog/tasks/task-{index}.md")), format!("---\nid: TASK-{index}\ntitle: Missing {index}\nstatus: To Do\ncreated_date: '{value}'\n---\n")).expect("missing date task");
    }
    h.app = open_app(&h.root, &h.config_path);
    h.terminal = Terminal::new(TestBackend::new(140, 30)).expect("terminal");
    h.render();
    h
}

fn paint(h: &mut Harness, bucket: &str, color: &str) {
    h.press(KeyCode::Char('p'));
    pick(h, "When task filed");
    pick(h, bucket);
    pick(h, color);
    h.press(KeyCode::Esc);
}

#[test]
fn utc_calendar_boundaries_overlap_without_treating_missing_as_recent() {
    let mut h = dated();
    paint(&mut h, "last 7 days", "green");
    for age in [0, 1, 6] {
        assert_eq!(
            cell_fg(&h, &format!("Age {age} boundary")),
            Some(Color::Green)
        );
    }
    for age in [-1, 7, 29, 30] {
        assert_ne!(
            cell_fg(&h, &format!("Age {age} boundary")),
            Some(Color::Green)
        );
    }
    for id in [8, 9, 10] {
        assert_ne!(cell_fg(&h, &format!("Missing {id}")), Some(Color::Green));
    }
    paint(&mut h, "missing", "magenta");
    for id in [8, 9, 10] {
        assert_eq!(cell_fg(&h, &format!("Missing {id}")), Some(Color::Magenta));
    }
    h.press(KeyCode::Esc);
    h.type_text("/filed:last30days");
    h.press(KeyCode::Enter);
    assert_eq!(h.app.visible.len(), 5);
    let screen = h.render();
    assert!(!screen.contains("Age 30 boundary"), "{screen}");
}

#[test]
fn independent_date_rules_follow_existing_row_column_precedence_and_restart() {
    let mut h = dated();
    paint(&mut h, "today", "red");
    assert_eq!(cell_fg(&h, "Age 0 boundary"), Some(Color::Red));
    // Persisted rules are the public configuration interface used by the same app.
    std::fs::write(h.root.join("views-repo.lua"), "return { [1] = { columns = 'id,title', paint = 'by:filed=today:red;rows:filed:last7days=green;column:id=cyan' } }").expect("view config");
    h.app = open_app(&h.root, &h.config_path);
    h.render();
    assert_eq!(cell_fg(&h, "Age 0 boundary"), Some(Color::Green));
    assert_eq!(
        cell_fg(&h, "2   "),
        Some(Color::Cyan),
        "id column overrides whole-row paint"
    );
    assert_eq!(cell_fg(&h, "Age 6 boundary"), Some(Color::Green));
    let task_state = h.app.state.clone();
    h.type_text("vsd");
    h.press(KeyCode::Tab);
    let menu = h.press(KeyCode::Char('p'));
    assert!(menu.contains("When merged"), "{menu}");
    assert!(!menu.contains("When task filed"), "{menu}");
    pick(&mut h, "When merged");
    pick(&mut h, "last 30 days");
    pick(&mut h, "blue");
    h.press(KeyCode::Esc);
    h.type_text("vsd");
    let pr_state = h.app.state.clone();
    h.app = open_app(&h.root, &h.config_path);
    assert_eq!(h.app.state, task_state);
    h.press(KeyCode::Tab);
    assert_eq!(h.app.state, pr_state);
    assert!(std::fs::read_to_string(h.root.join("views-repo.prs.lua"))
        .expect("PR view")
        .contains("by:merged=last30days:blue"));
}

#[test]
fn date_picker_is_available_with_zero_rows_and_at_narrow_and_wide_sizes() {
    for width in [42, 80, 140] {
        let mut h = dated();
        h.terminal = Terminal::new(TestBackend::new(width, 16)).expect("terminal");
        h.type_text("/no-such-task");
        h.press(KeyCode::Enter);
        h.press(KeyCode::Char('p'));
        pick(&mut h, "When task filed");
        let screen = h.render();
        assert!(screen.contains("last 7 days"), "{screen}");
        pick(&mut h, "future");
        pick(&mut h, "blue");
        h.press(KeyCode::Esc);
        assert_eq!(h.app.state.paint[0].to_text(), "by:filed=future:blue");
        assert_eq!(h.app.visible.len(), 0);
    }
}

#[test]
#[ignore = "requires authenticated GitHub and SBT_PR_REPO; read-only"]
fn authoritative_live_merge_dates_paint_real_pr_rows_and_survive_restart() {
    use std::time::{Duration as Wait, Instant};
    let repo = std::env::var_os("SBT_PR_REPO").expect("live repo");
    let snapshot =
        switchbard_core::fetch_pull_requests(std::path::Path::new(&repo)).expect("GitHub history");
    let mut h = Harness::new();
    for args in [
        vec!["init", "-q"],
        vec!["remote", "add", "origin", &snapshot.repository_url],
    ] {
        assert!(std::process::Command::new("git")
            .arg("-C")
            .arg(&h.root)
            .args(args)
            .status()
            .expect("git")
            .success());
    }
    h.terminal = Terminal::new(TestBackend::new(140, 30)).expect("terminal");
    h.press(KeyCode::Tab);
    let until = Instant::now() + Wait::from_secs(100);
    while h.app.pull_requests.loading() && Instant::now() < until {
        std::thread::sleep(Wait::from_millis(20));
        h.app.tick();
    }
    assert!(
        h.app.pull_requests.error.is_none(),
        "{:?}",
        h.app.pull_requests.error
    );
    h.press(KeyCode::Char('p'));
    pick(&mut h, "When merged");
    pick(&mut h, "last 30 days");
    pick(&mut h, "green");
    h.press(KeyCode::Esc);
    h.type_text("/status:merged merged:last30days");
    h.press(KeyCode::Enter);
    let row = h.app.pull_requests.row().expect("recent merged PR").clone();
    assert!(row.merged_at.is_some());
    h.render();
    assert_eq!(cell_fg(&h, &row.number.to_string()), Some(Color::Green));
    h.type_text("vsd");
    let saved = h.app.state.clone();
    h.app = open_app(&h.root, &h.config_path);
    h.press(KeyCode::Tab);
    assert_eq!(h.app.state, saved);
}
