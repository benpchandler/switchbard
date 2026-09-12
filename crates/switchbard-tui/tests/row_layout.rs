//! Configurable task rows through the real input, persistence and terminal renderer.
mod harness;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use harness::*;
use ratatui::{backend::TestBackend, Terminal};
use switchbard_tui::{
    columns::{Column, ColumnRegistry},
    row_layout::RowLayout,
    views::ViewState,
};

#[test]
fn settings_preview_save_switch_and_resume_keep_layout_scoped_to_view() {
    let mut h = Harness::new();
    let screen = h.type_text(",w");
    assert!(screen.contains("up to 2 lines (this view)"), "{screen}");
    h.type_text("s");
    assert!(h.app.state.row_layout.spaced);
    assert!(h.app.status.contains("v s saves"));
    h.press(KeyCode::Esc);
    let layout = h.app.state.row_layout;
    let resume = h.app.resume_state();
    assert_eq!(
        open_app(&h.root, &h.config_path).state.row_layout,
        RowLayout::default()
    );
    h.app = open_app(&h.root, &h.config_path);
    h.app.resume_from(Some(&resume));
    assert_eq!(h.app.state.row_layout, layout);
    h.type_text("vsd");
    assert_eq!(open_app(&h.root, &h.config_path).state.row_layout, layout);
    h.type_text("v2");
    assert_eq!(h.app.state.row_layout, RowLayout::default());
    h.type_text("v1");
    assert_eq!(h.app.state.row_layout, layout);
    h.next_list_page();
    assert_eq!(h.app.state.row_layout, RowLayout::default());
    let pr_settings = h.press(KeyCode::Char(','));
    assert!(!pr_settings.contains("Title wrapping:"));
    assert!(!pr_settings.contains("Row spacing:"));
    h.press(KeyCode::Esc);
    h.next_list_page();
    assert_eq!(h.app.state.row_layout, layout);
    h.type_text(",wwwwss");
    assert_eq!(
        h.app.state.row_layout, layout,
        "cycles return to prior settings"
    );
}

#[test]
fn old_views_stay_compact_and_invalid_layout_records_are_preserved() {
    let registry = ColumnRegistry::builtin_only();
    assert_eq!(
        ViewState::from_lua("{ title_lines=2.0 }", &registry)
            .row_layout
            .lines(),
        2
    );
    assert_eq!(
        ViewState::from_lua("{ columns='id,title' }", &registry).row_layout,
        RowLayout::default()
    );
    for fields in [
        "title_lines=0",
        "title_lines=2.5",
        "title_lines=-1",
        "title_lines=math.huge",
        "title_lines='2'",
        "row_spacing=0.5",
        "title_lines=65536",
        "title_lines='bad'",
        "row_spacing=2",
    ] {
        let mut h = Harness::new();
        let source = format!("return {{ [1] = {{ {fields} }} }}");
        let path = h.root.join("views-repo.lua");
        std::fs::write(&path, &source).unwrap();
        h.app = open_app(&h.root, &h.config_path);
        let screen = h.type_text("vsd");
        assert!(screen.contains("repair file and reopen"), "{screen}");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), source);
    }
}

#[test]
fn pr_view_layout_is_unsupported_without_overwriting_the_source() {
    let mut h = Harness::new();
    let path = h.root.join("views-repo.prs.lua");
    let source = "return { [1] = { columns='id,title', title_lines=3, row_spacing=1 } }";
    std::fs::write(&path, source).unwrap();
    h.app = open_app(&h.root, &h.config_path);
    h.next_list_page();
    assert_eq!(h.app.state.row_layout, RowLayout::default());
    let screen = h.type_text("vsd");
    assert!(screen.contains("repair file and reopen"), "{screen}");
    assert_eq!(std::fs::read_to_string(&path).unwrap(), source);
}

fn narrow_fixture() -> Harness {
    let mut h = Harness::new();
    seed(
        &h.root,
        "Review the configurable task row layout with wrapping and breathing room",
        "To Do",
        &[],
    );
    seed(
        &h.root,
        "Check selection across wrapped rows and preserve the chosen task after resize",
        "To Do",
        &[],
    );
    h.press(KeyCode::Char('r'));
    h.press(KeyCode::Esc);
    h.app.state.columns = vec![Column::Id, Column::Status, Column::Title];
    h.terminal = Terminal::new(TestBackend::new(58, 22)).unwrap();
    h
}

#[test]
fn wrapped_titles_and_blank_spacing_render_real_narrow_rows() {
    let mut h = narrow_fixture();
    let compact = h.render();
    assert!(!compact.contains("breathing room"), "{compact}");
    export(&h, "compact");
    h.type_text(",wws");
    h.press(KeyCode::Esc);
    let wrapped = h.render();
    assert!(wrapped.contains("breathing room"), "{wrapped}");
    export(&h, "wrapped-spaced");
    let lines: Vec<_> = wrapped.lines().collect();
    let first = lines
        .iter()
        .position(|line| line.contains("Fix login redirect"))
        .unwrap();
    assert!(
        lines[first + 1].trim_matches(['│', ' ']).is_empty(),
        "{wrapped}"
    );
    assert!(h.app.page_size < 17, "page navigation counts task rows");
}

#[test]
fn capped_unicode_titles_navigation_grouping_and_tiny_viewports_are_bounded() {
    let mut h = narrow_fixture();
    seed(
        &h.root,
        &format!("Unicode 東京 e\u{301} 👩‍💻 {}", "unbroken".repeat(100)),
        "To Do",
        &[],
    );
    h.press(KeyCode::Char('r'));
    h.press(KeyCode::Esc);
    h.type_text(",wwws");
    h.press(KeyCode::Esc);
    h.type_text("/Unicode");
    h.press(KeyCode::Enter);
    let screen = h.render();
    assert!(screen.contains("Unicode 東京"), "{screen}");
    assert!(
        screen.contains('…'),
        "capped title has overflow indicator: {screen}"
    );
    export(&h, "unicode-capped");
    for (width, height) in [(58, 9), (32, 6), (100, 30), (1, 1), (0, 0), (58, 22)] {
        h.terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        let selected = h.app.selected_task().unwrap().id.clone();
        h.render();
        assert_eq!(h.app.selected_task().unwrap().id, selected);
        if width >= 58 {
            assert!(h.render().contains("Unicode"));
        }
    }
    h.type_text("/");
    h.press(KeyCode::Enter);
    h.type_text("o1");
    h.type_text("/Unicode");
    h.press(KeyCode::Enter);
    assert!(h.render().contains("Unicode"));
    h.press(KeyCode::Char('k'));
    h.press(KeyCode::Char('j'));
    assert!(h.render().contains("Unicode"));
    h.type_text("/no-matching-task");
    h.press(KeyCode::Enter);
    assert!(h.app.selected_task().is_none());
    h.press(KeyCode::Char('j'));
}

#[test]
fn capped_title_shows_overflow_when_zero_width_remainder_fits() {
    let mut h = narrow_fixture();
    let title = format!("Capped{}", "\u{301}".repeat(4091));
    std::fs::write(
        h.root.join("backlog/tasks/task-3.md"),
        format!("---\nid: TASK-3\ntitle: {title}\nstatus: To Do\npriority: medium\n---\n"),
    )
    .unwrap();
    h.press(KeyCode::Char('r'));
    h.press(KeyCode::Esc);
    h.type_text(",w");
    h.press(KeyCode::Esc);
    h.type_text("/Capped");
    h.press(KeyCode::Enter);
    assert!(h.render().contains('…'));
}

#[test]
fn many_wrapped_tasks_scroll_by_visible_tasks_and_keep_selection_after_resize() {
    let mut h = Harness::new();
    for index in 0..250 {
        seed(
            &h.root,
            &format!("Item{index:03} with a title long enough to wrap in the narrow terminal pane"),
            "To Do",
            &[],
        );
    }
    h.press(KeyCode::Char('r'));
    h.press(KeyCode::Esc);
    h.app.state.columns = vec![Column::Id, Column::Title];
    h.type_text(",ws");
    h.press(KeyCode::Esc);
    for (width, height) in [(48, 16), (80, 22), (35, 6)] {
        h.terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        h.press(KeyCode::Char('G'));
        assert!(h
            .render()
            .contains(h.selected_title().split_whitespace().next().unwrap()));
        h.press(KeyCode::Char('g'));
        let before = h.app.selected;
        let page = h.app.page_size;
        h.app
            .handle_key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::CONTROL));
        h.render();
        assert_eq!(h.app.selected, before + page);
        let down = h.app.selected;
        let up_page = h.app.page_size;
        h.app
            .handle_key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL));
        h.render();
        assert_eq!(h.app.selected, down.saturating_sub(up_page));
        let kept = h.app.selected_task().unwrap().id.clone();
        h.type_text(",w");
        h.press(KeyCode::Esc);
        assert_eq!(h.app.selected_task().unwrap().id, kept);
        assert!(h
            .render()
            .contains(h.selected_title().split_whitespace().next().unwrap()));
    }
    h.terminal = Terminal::new(TestBackend::new(100, 20)).unwrap();
    h.type_text("o1");
    h.press(KeyCode::Char('g'));
    h.press(KeyCode::Enter);
    let before = h.app.selected;
    let page = h.app.page_size;
    h.app
        .handle_key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::CONTROL));
    let screen = h.render();
    assert_eq!(h.app.selected, before + page);
    assert!(screen.contains(h.selected_title().split_whitespace().next().unwrap()));
}

#[test]
fn spacing_never_hides_selected_task_or_an_otherwise_fitting_group_heading() {
    let mut h = Harness::new();
    seed_project(&h.root, "Tiny project", "Planned", None);
    seed_in_project(
        &h.root,
        "Visible selected task",
        "To Do",
        "Tiny project",
        None,
    );
    h.press(KeyCode::Char('r'));
    h.press(KeyCode::Esc);
    h.type_text("o1");
    h.press(KeyCode::Char('g'));
    h.type_text(",ws");
    h.press(KeyCode::Esc);
    for (height, heading) in [(6, false), (7, true)] {
        h.terminal = Terminal::new(TestBackend::new(100, height)).unwrap();
        let screen = h.render();
        assert!(screen.contains("Visible selected task"), "{screen}");
        assert_eq!(screen.contains("Tiny project"), heading, "{screen}");
    }
}

#[test]
fn selection_band_covers_continuation_lines_but_leaves_spacing_blank() {
    let mut h = narrow_fixture();
    h.type_text(",wws");
    h.press(KeyCode::Esc);
    h.type_text("/Review");
    h.press(KeyCode::Enter);
    let screen = h.render();
    let y = screen
        .lines()
        .position(|line| line.contains("Review the configurable"))
        .unwrap() as u16;
    let buffer = h.terminal.backend().buffer();
    let selected = h
        .app
        .config
        .theme
        .style(switchbard_tui::config::Surface::Selected)
        .bg
        .unwrap();
    assert_eq!(buffer[(17, y)].bg, selected);
    assert_eq!(buffer[(17, y + 1)].bg, selected);
    assert_eq!(
        buffer[(1, y + 1)].bg,
        selected,
        "full-row band remains coherent"
    );
    assert_ne!(
        buffer[(17, y + 2)].bg,
        selected,
        "spacing is separate from selection"
    );
}

fn export(h: &Harness, name: &str) {
    let Ok(path) = std::env::var("SBT_ROW_LAYOUT_EVIDENCE") else {
        return;
    };
    let buffer = h.terminal.backend().buffer();
    let cells: Vec<_> = buffer.content.iter().map(|cell| serde_json::json!({
        "symbol": cell.symbol(), "width": ratatui::text::Span::raw(cell.symbol()).width(), "fg": format!("{:?}", cell.fg), "bg": format!("{:?}", cell.bg),
    })).collect();
    std::fs::create_dir_all(&path).unwrap();
    std::fs::write(
        std::path::Path::new(&path).join(format!("{name}.json")),
        serde_json::to_vec_pretty(&serde_json::json!({
            "width": buffer.area.width, "height": buffer.area.height, "cells": cells,
        }))
        .unwrap(),
    )
    .unwrap();
}
