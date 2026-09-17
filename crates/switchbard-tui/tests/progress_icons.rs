//! Visual criterion coverage uses the same cached aggregate as Checklist.
mod harness;

use crossterm::event::KeyCode;
use harness::{open_app, select_task_titled, Harness};
use ratatui::{backend::TestBackend, Terminal};
use switchbard_core::{
    edit_backlog_task, load_backlog_repo, revise_backlog_acceptance_criteria,
    set_backlog_acceptance_checked, BacklogTaskPatch,
};
use switchbard_tui::columns::Column;

fn evidence(h: &Harness, name: &str) {
    let Ok(directory) = std::env::var("SBT_PROGRESS_EVIDENCE_DIR") else {
        return;
    };
    let buffer = h.terminal.backend().buffer();
    let cells: Vec<_> = buffer
        .content
        .iter()
        .map(|cell| {
            serde_json::json!({
                "symbol": cell.symbol(), "fg": format!("{:?}", cell.fg),
                "bg": format!("{:?}", cell.bg), "modifier": format!("{:?}", cell.modifier)
            })
        })
        .collect();
    std::fs::create_dir_all(&directory).unwrap();
    std::fs::write(std::path::Path::new(&directory).join(format!("{name}.json")),
        serde_json::to_vec(&serde_json::json!({"width": buffer.area.width, "height": buffer.area.height, "cells": cells})).unwrap()).unwrap();
}

fn coverage(h: &Harness, id: &str, total: usize, checked: usize) {
    let repo = load_backlog_repo(&h.root).unwrap();
    let task = repo.tasks.iter().find(|task| task.id == id).unwrap();
    let remove: Vec<usize> = (1..=task.acceptance_criteria.len()).collect();
    revise_backlog_acceptance_criteria(&h.root, id, &[], &remove).unwrap();
    edit_backlog_task(
        &h.root,
        id,
        &BacklogTaskPatch {
            append_acceptance_criteria: (0..total).map(|i| format!("Outcome {i}")).collect(),
            ..Default::default()
        },
    )
    .unwrap();
    for index in 1..=checked {
        set_backlog_acceptance_checked(&h.root, id, index, true).unwrap();
    }
}

fn show_progress(h: &mut Harness) {
    h.type_text("cprog");
    h.press(KeyCode::Esc);
    assert!(h.app.state.columns.contains(&Column::Progress));
    // Move the appended Progress next to Status, keeping Title at the end.
    h.type_text("cm125");
    h.press(KeyCode::Enter);
    h.press(KeyCode::Esc);
}

fn row_icon(h: &mut Harness, title: &str, icon: &str) {
    let screen = h.render();
    // TASK-235's filter line ("│/ <filter>") can itself contain `title` when
    // the caller filtered by it (`select_task_titled`); skip it so this finds
    // the actual task row, not the filter echoing the same text above it.
    let row = screen
        .lines()
        .find(|line| {
            line.strip_prefix('│')
                .is_some_and(|rest| !rest.starts_with('/') && rest.contains(title))
        })
        .unwrap();
    assert!(row.contains(icon), "expected {icon}: {row}");
}

#[test]
fn measured_boundaries_render_without_rounding_to_empty_or_complete() {
    let mut h = Harness::new();
    show_progress(&mut h);
    coverage(&h, "TASK-1", 100, 0);
    let mut checked = 0;
    for (target, icon) in [
        (0, "████"),
        (1, "▏███"),
        (33, "█▎██"),
        (34, "█▎██"),
        (66, "██▋█"),
        (67, "██▋█"),
        (99, "███▉"),
        (100, "████"),
    ] {
        for index in checked + 1..=target {
            set_backlog_acceptance_checked(&h.root, "TASK-1", index, true).unwrap();
        }
        checked = target;
        h.press(KeyCode::Char('r'));
        h.tick_until_tasks_settle();
        row_icon(&mut h, "Fix login", icon);
        let screen = h.render();
        let y = screen
            .lines()
            .position(|line| line.contains("Fix login"))
            .unwrap() as u16;
        let buffer = h.terminal.backend().buffer();
        let x = (0..buffer.area.width)
            .find(|x| buffer[(*x, y)].symbol() == "\u{e0b6}")
            .unwrap();
        let fill = if target == 100 {
            ratatui::style::Color::Rgb(255, 86, 95)
        } else {
            ratatui::style::Color::Rgb(82, 143, 255)
        };
        let shell = ratatui::style::Color::Rgb(38, 62, 97);
        assert_eq!(buffer[(x, y)].fg, if target == 0 { shell } else { fill });
        assert_eq!(
            buffer[(x + 5, y)].fg,
            if target == 100 { fill } else { shell }
        );
        if target == 0 || target == 100 {
            for offset in 1..5 {
                assert_eq!(buffer[(x + offset, y)].fg, buffer[(x, y)].fg);
                assert_eq!(buffer[(x + offset, y)].bg, buffer[(x, y)].bg);
            }
        }
        evidence(&h, &format!("pill-boundary-{target}"));
    }
    assert_eq!(
        load_backlog_repo(&h.root)
            .unwrap()
            .tasks
            .iter()
            .find(|t| t.id == "TASK-1")
            .unwrap()
            .status,
        "In Progress"
    );
    // 100/101 must retain an incomplete icon even if a presentation rounds it.
    edit_backlog_task(
        &h.root,
        "TASK-1",
        &BacklogTaskPatch {
            append_acceptance_criteria: vec!["One more".into()],
            ..Default::default()
        },
    )
    .unwrap();
    h.press(KeyCode::Char('r'));
    h.tick_until_tasks_settle();
    row_icon(&mut h, "Fix login", "███▉");
    coverage(&h, "TASK-2", 201, 1);
    coverage(&h, "TASK-3", 0, 0);
    h.press(KeyCode::Char('r'));
    h.tick_until_tasks_settle();
    row_icon(&mut h, "Add dark", "▏███");
    h.type_text("vp");
    h.type_text("sprog");
    h.press(KeyCode::Char('a'));
    assert_eq!(h.app.state.sort.unwrap().column, Column::Progress);
    assert_eq!(
        harness::screen_rows(&h),
        vec![
            "Add dark theme",
            "Fix login redirect loop",
            "Write onboarding guide"
        ]
    );
    h.type_text("sprog");
    h.press(KeyCode::Char('d'));
    assert_eq!(
        harness::screen_rows(&h),
        vec![
            "Fix login redirect loop",
            "Add dark theme",
            "Write onboarding guide"
        ]
    );
}

#[test]
fn descendants_canceled_and_manual_done_share_checklist_truth() {
    let mut h = Harness::new();
    show_progress(&mut h);
    harness::seed_project(&h.root, "Coverage", "In Progress", None);
    let config = h.root.join("backlog/config.yml");
    let text = std::fs::read_to_string(&config).unwrap();
    std::fs::write(config, text.replace("\"Done\"]", "\"Done\", \"Canceled\"]")).unwrap();
    coverage(&h, "TASK-1", 0, 0);
    for title in ["Four outcomes A", "Four outcomes B", "Canceled outcomes"] {
        harness::seed_in_project(&h.root, title, "To Do", "Coverage", Some("TASK-1"));
        let repo = load_backlog_repo(&h.root).unwrap();
        let child = repo.tasks.iter().find(|t| t.title == title).unwrap();
        coverage(&h, &child.id, 4, usize::from(title.ends_with('A')));
        if title.starts_with("Canceled") {
            edit_backlog_task(
                &h.root,
                &child.id,
                &BacklogTaskPatch {
                    status: Some("Canceled".into()),
                    ..Default::default()
                },
            )
            .unwrap();
        }
    }
    coverage(&h, "TASK-3", 0, 0);
    edit_backlog_task(
        &h.root,
        "TASK-2",
        &BacklogTaskPatch {
            status: Some("Done".into()),
            ..Default::default()
        },
    )
    .unwrap();
    h.press(KeyCode::Char('r'));
    h.tick_until_tasks_settle();
    row_icon(&mut h, "Fix login", "▌███");
    row_icon(&mut h, "Add dark", "████");
    row_icon(&mut h, "Write onboarding", "-");
    row_icon(&mut h, "Canceled outcomes", "-");
    select_task_titled(&mut h, "Fix login redirect loop");
    let screen = h.press(KeyCode::Enter);
    assert!(screen.contains("1/8 12.5%"), "{screen}");
}

#[test]
fn default_adjacency_saved_view_picker_custom_glyph_and_layouts() {
    let mut h = Harness::new();
    assert!(!h.app.state.columns.contains(&Column::Progress));
    show_progress(&mut h);
    assert_eq!(
        h.app.state.columns[1..3],
        [Column::Status, Column::Progress]
    );
    h.type_text("vs1");
    h.app = open_app(&h.root, &h.config_path);
    assert_eq!(
        h.app.state.columns[1..3],
        [Column::Status, Column::Progress]
    );
    std::fs::write(
        &h.config_path,
        "return { glyphs = { progress = { empty = '□' } } }",
    )
    .unwrap();
    h.app = open_app(&h.root, &h.config_path);
    row_icon(&mut h, "Fix login", "□");
    std::fs::remove_file(&h.config_path).unwrap();
    std::fs::remove_file(h.root.join("views.lua")).unwrap();
    std::fs::remove_file(h.root.join("views-repo.lua")).unwrap();
    h.app = open_app(&h.root, &h.config_path);
    let status = h
        .app
        .state
        .columns
        .iter()
        .position(|c| *c == Column::Status)
        .unwrap();
    assert_eq!(h.app.state.columns[status + 1], Column::Progress);
    coverage(&h, "TASK-1", 1, 1);
    coverage(&h, "TASK-2", 8, 1);
    coverage(&h, "TASK-3", 0, 0);
    h.press(KeyCode::Char('r'));
    h.tick_until_tasks_settle();
    for (title, total, checked) in [
        ("No checked outcomes", 1, 0),
        ("Half checked outcomes", 2, 1),
        ("Mostly checked outcomes", 4, 3),
    ] {
        harness::seed(&h.root, title, "In Progress", &[]);
        let repo = load_backlog_repo(&h.root).unwrap();
        let task = repo.tasks.iter().find(|t| t.title == title).unwrap();
        coverage(&h, &task.id, total, checked);
    }
    h.press(KeyCode::Char('r'));
    h.tick_until_tasks_settle();
    for (width, height) in [(140, 24), (100, 20), (60, 12), (40, 8)] {
        h.terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        let screen = h.render();
        assert!(screen.contains("████") || screen.contains("●"), "{screen}");
        evidence(&h, &format!("progress-{width}x{height}"));
        println!("EVIDENCE progress {width}x{height}\n{screen}\nEND EVIDENCE");
    }
    h.terminal = Terminal::new(TestBackend::new(140, 24)).unwrap();
    let screen = h.render();
    assert!(screen.contains("Review 1/1 100%"), "{screen}");
    select_task_titled(&mut h, "Fix login redirect loop");
    let screen = h.press(KeyCode::Enter);
    assert!(screen.contains("checklist: Review 1/1 100%"), "{screen}");
    evidence(&h, "progress-detail");
}

#[test]
fn compact_progress_header_keeps_percent_at_two_digit_positions() {
    let mut h = Harness::new();
    std::fs::write(h.root.join("views.lua"),
        "return {{ columns = 'id,status,priority,planning,checklist,labels,project,ball,blocked,progress,title' }}").unwrap();
    h.app = open_app(&h.root, &h.config_path);
    h.terminal = Terminal::new(TestBackend::new(180, 20)).unwrap();
    let screen = h.press(KeyCode::Char('r'));
    h.tick_until_tasks_settle();
    assert!(harness::header_line(&screen).contains("10 %"), "{screen}");
    row_icon(&mut h, "Fix login", "████");
}

#[test]
fn pill_theme_selection_working_and_ascii_fallback() {
    use ratatui::style::Color;
    let mut h = Harness::new();
    show_progress(&mut h);
    coverage(&h, "TASK-1", 8, 4);
    h.press(KeyCode::Char('r'));
    h.tick_until_tasks_settle();
    select_task_titled(&mut h, "Fix login redirect loop");
    for (theme, color) in [
        ("berg", Color::Rgb(82, 143, 255)),
        ("light", Color::Rgb(36, 93, 204)),
        ("plain", Color::Reset),
    ] {
        h.type_text(&format!(":theme {theme}"));
        h.press(KeyCode::Enter);
        let body = if theme == "plain" {
            "██░░"
        } else {
            "████"
        };
        row_icon(&mut h, "Fix login", &format!("\u{e0b6}{body}\u{e0b4}"));
        let buffer = h.terminal.backend().buffer();
        let index = buffer
            .content
            .iter()
            .position(|c| c.symbol() == "\u{e0b6}")
            .unwrap();
        assert_eq!(buffer.content[index].fg, color);
        assert_eq!(buffer.content[index].bg, buffer.content[index - 1].bg);
        evidence(&h, &format!("pill-{theme}-selected"));
    }
    std::fs::write(
        &h.config_path,
        "return { progress_style = 'ascii', work = { period_ms = 0 } }",
    )
    .unwrap();
    h.app.tick();
    row_icon(&mut h, "Fix login", "(##..)");
    switchbard_core::claim_work(
        &h.root.join("work"),
        &switchbard_core::WorkIdentity {
            session_id: "pill-working".into(),
            pid: std::process::id(),
            agent: "claude".into(),
        },
        &h.root,
        "TASK-1",
    )
    .unwrap();
    h.app.tick();
    let screen = h.render();
    assert_eq!(h.app.working_sessions(), 1, "{screen}");
    // TASK-241: full glow (period_ms = 0) is berg's brightened working-pulse
    // peak, not its declared rest color (`oklch::WORK_LIGHTNESS_SWING_DARK`).
    assert_eq!(
        harness::cell_bg(&h, "(##..)"),
        Some(Color::Rgb(0x4d, 0x72, 0x66))
    );
    evidence(&h, "pill-ascii-working");
    std::fs::write(
        &h.config_path,
        "return { progress_style = 'pill', glyphs = { progress = { empty = 'x' } } }",
    )
    .unwrap();
    h.app.tick();
    row_icon(&mut h, "Fix login", "\u{e0b6}████\u{e0b4}");
    evidence(&h, "pill-working");
}

#[test]
fn unmeasured_only_view_stays_a_dash_when_column_is_compact() {
    let mut h = Harness::new();
    show_progress(&mut h);
    std::fs::write(&h.config_path, "return { progress_style = 'pill', glyphs = { progress = { unmeasured = 'X', complete = 'X' } } }").unwrap();
    h.app.tick();
    for id in ["TASK-1", "TASK-2", "TASK-3"] {
        coverage(&h, id, 0, 0);
    }
    h.press(KeyCode::Char('r'));
    for width in [100, 40] {
        h.terminal = Terminal::new(TestBackend::new(width, 12)).unwrap();
        let screen = h.render();
        let buffer = h.terminal.backend().buffer();
        assert_eq!(
            buffer
                .content
                .iter()
                .filter(|cell| cell.symbol() == "-")
                .count(),
            3,
            "{screen}"
        );
        assert!(
            !screen.contains('\u{e0b6}')
                && !screen
                    .lines()
                    .filter(|line| line.starts_with('│'))
                    .any(|line| line.contains('X')),
            "{screen}"
        );
        evidence(&h, &format!("pill-unmeasured-{width}"));
    }
}

#[test]
fn ascii_compact_fallback_uses_ascii_endpoints() {
    let mut h = Harness::new();
    show_progress(&mut h);
    std::fs::write(&h.config_path, "return { progress_style = 'ascii' }").unwrap();
    h.app.tick();
    coverage(&h, "TASK-1", 2, 1);
    coverage(&h, "TASK-2", 1, 0);
    coverage(&h, "TASK-3", 1, 1);
    h.press(KeyCode::Char('r'));
    h.tick_until_tasks_settle();
    h.terminal = Terminal::new(TestBackend::new(40, 12)).unwrap();
    let screen = h.render();
    row_icon(&mut h, "Fix login", "+");
    row_icon(&mut h, "Add dark", "0");
    row_icon(&mut h, "Write onboarding", "#");
    assert!(
        !screen.contains('◑') && !screen.contains('●') && !screen.contains('○'),
        "{screen}"
    );
    evidence(&h, "pill-ascii-narrow");
}

#[test]
fn complete_color_and_clean_shell_preserve_painted_rows() {
    use ratatui::style::Color;
    let mut h = Harness::new();
    show_progress(&mut h);
    select_task_titled(&mut h, "Fix login redirect loop");
    h.type_text(":paint rows:id:TASK-1=h2");
    h.press(KeyCode::Enter);
    for (theme, complete) in [
        ("berg", Color::Rgb(255, 86, 95)),
        ("light", Color::Rgb(184, 30, 53)),
    ] {
        h.type_text(&format!(":theme {theme}"));
        h.press(KeyCode::Enter);
        for checked in [0, 1] {
            coverage(&h, "TASK-1", 1, checked);
            h.press(KeyCode::Char('r'));
            h.tick_until_tasks_settle();
            h.type_text(&format!(":theme {theme}"));
            h.press(KeyCode::Enter);
            row_icon(&mut h, "Fix login", "\u{e0b6}████\u{e0b4}");
            let buffer = h.terminal.backend().buffer();
            let index = buffer
                .content
                .iter()
                .position(|c| c.symbol() == "\u{e0b6}")
                .unwrap();
            let cap = &buffer.content[index];
            assert_eq!(cap.bg, buffer.content[index - 1].bg);
            for offset in 1..6 {
                assert_eq!(buffer.content[index + offset].fg, cap.fg);
                assert_eq!(buffer.content[index + offset].bg, cap.bg);
            }
            if checked == 1 {
                assert_eq!(cap.fg, complete);
            }
            evidence(&h, &format!("pill-{theme}-painted-{checked}"));
        }
    }
}

#[test]
fn legacy_custom_theme_retains_fill_unless_completion_is_configured() {
    use ratatui::style::Color;
    let mut h = Harness::new();
    show_progress(&mut h);
    coverage(&h, "TASK-1", 1, 1);
    select_task_titled(&mut h, "Fix login redirect loop");
    for (completion, expected) in [
        ("", Color::Rgb(10, 80, 150)),
        (
            ", progress_complete = { fg = '#dd2233' }",
            Color::Rgb(221, 34, 51),
        ),
        (", progress_complete = {}", Color::Reset),
    ] {
        std::fs::write(&h.config_path, format!("return {{ theme = 'custom', themes = {{ custom = {{ progress_fill = {{ fg = '#0a5096' }}, progress_shell = {{ fg = '#123456' }} {completion} }} }} }}")).unwrap();
        h.press(KeyCode::Char('r'));
        h.tick_until_tasks_settle();
        row_icon(&mut h, "Fix login", "\u{e0b6}████\u{e0b4}");
        assert_eq!(harness::cell_fg(&h, "\u{e0b6}"), Some(expected));
    }
}
