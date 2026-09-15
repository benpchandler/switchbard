//! Real input and rendered task detail scrolling.
mod harness;

use crossterm::event::KeyCode;
use harness::*;
use switchbard_core::{create_task_allocating_id, NewBacklogTask};

fn long_detail() -> Harness {
    let mut h = Harness::new();
    create_task_allocating_id(
        &h.root,
        &NewBacklogTask {
            title: "Long detail fixture".into(),
            description: (0..80)
                .map(|n| {
                    if n == 40 {
                        format!("Detail line {n:02} 日本語 café {}\n", "unbroken".repeat(40))
                    } else {
                        format!("Detail line {n:02}\n")
                    }
                })
                .collect(),
            status: "To Do".into(),
            priority: "medium".into(),
            acceptance_criteria: vec!["End of long detail".into()],
            parent: None,
            labels: vec!["scroll-fixture".into()],
            assignees: vec![],
            project: None,
            dependencies: vec![],
            due_date: None,
            custom: vec![],
        },
    )
    .unwrap();
    h.app.tick();
    h.press(KeyCode::Char('/'));
    h.type_text("label:scroll-fixture");
    h.press(KeyCode::Enter);
    h
}

#[test]
fn enter_focuses_details_and_down_scrolls_without_changing_task() {
    let mut h = long_detail();
    let before = h.press(KeyCode::Enter);
    assert!(before.contains("Detail line 00"), "{before}");
    let selected = h.selected_title();
    for _ in 0..20 {
        h.press(KeyCode::Down);
    }
    let after = h.render();
    assert_eq!(h.selected_title(), selected);
    assert!(after.contains("Detail line 20"), "{after}");
    assert!(!after.contains("Detail line 00"), "{after}");
}

fn mouse(h: &mut Harness, kind: crossterm::event::MouseEventKind, column: u16, row: u16) -> String {
    h.app.handle_mouse(crossterm::event::MouseEvent {
        kind,
        column,
        row,
        modifiers: crossterm::event::KeyModifiers::NONE,
    });
    h.render()
}

#[test]
fn shift_tab_returns_to_list_and_back_without_closing_or_losing_scroll() {
    let mut h = long_detail();
    h.press(KeyCode::Char('/'));
    h.press(KeyCode::Esc);
    h.press(KeyCode::Esc);
    h.press(KeyCode::End);
    h.press(KeyCode::Up);
    h.press(KeyCode::Enter);
    h.press(KeyCode::PageDown);
    let offset = h.app.detail_scroll;
    let selected = h.selected_title();
    let list = h.press(KeyCode::BackTab);
    assert!(list.contains("List active"), "{list}");
    assert!(list.contains("Detail line"), "{list}");
    let detail = h.press(KeyCode::BackTab);
    assert!(detail.contains("Detail active"), "{detail}");
    assert_eq!(h.app.detail_scroll, offset);
    assert_eq!(h.selected_title(), selected);
    h.press(KeyCode::BackTab);
    h.press(KeyCode::Up);
    assert_ne!(h.selected_title(), selected);
    assert_eq!(h.app.detail_scroll, 0);
    let closed = h.press(KeyCode::Esc);
    assert!(!closed.contains(" Detail "), "{closed}");
}

#[test]
fn pointer_scrolls_hovered_pane_and_click_changes_keyboard_focus() {
    use crossterm::event::{MouseButton, MouseEventKind::*};
    let mut h = long_detail();
    h.press(KeyCode::Enter);
    let selected = h.selected_title();
    let before = h.render();
    for _ in 0..20 {
        mouse(&mut h, ScrollDown, 75, 10);
    }
    let after = h.render();
    assert_ne!(before, after);
    assert!(after.contains("Detail line 20"), "{after}");
    assert_eq!(selected, h.selected_title());
    mouse(&mut h, Down(MouseButton::Left), 10, 10);
    assert!(!h.app.detail_focused());
    let offset = h.app.detail_scroll;
    mouse(&mut h, ScrollDown, 10, 10);
    assert_eq!(h.app.detail_scroll, offset);
    mouse(&mut h, Down(MouseButton::Left), 75, 10);
    assert!(h.app.detail_focused());
    mouse(&mut h, ScrollUp, 75, 10);
    assert_eq!(h.app.detail_scroll, offset - 1);
    let offset = h.app.detail_scroll;
    mouse(&mut h, ScrollDown, 75, 0);
    assert_eq!(h.app.detail_scroll, offset, "header ignores scroll");
}

#[test]
fn boundaries_resize_and_multiline_content_remain_scrollable() {
    use ratatui::{backend::TestBackend, Terminal};
    let mut h = long_detail();
    h.press(KeyCode::Enter);
    for (width, height) in [(40, 8), (100, 20), (180, 50), (1, 1), (0, 0), (100, 20)] {
        h.terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        h.render();
        h.press(KeyCode::End);
        for _ in 0..5 {
            h.press(KeyCode::PageDown);
        }
        let bottom = h.render();
        if width >= 100 {
            assert!(bottom.contains("End of long detail"), "{bottom}");
        }
        h.press(KeyCode::Home);
        for _ in 0..5 {
            h.press(KeyCode::Up);
        }
        assert_eq!(h.app.detail_scroll, 0);
        let top = h.render();
        if width >= 100 {
            assert!(top.contains("Detail line 00"), "{top}");
        }
    }
}

#[test]
fn modals_and_filter_own_input_before_detail_focus() {
    let mut h = long_detail();
    h.press(KeyCode::Enter);
    h.press(KeyCode::PageDown);
    let offset = h.app.detail_scroll;
    h.press(KeyCode::Char('f'));
    let picker = h.render();
    h.press(KeyCode::BackTab);
    mouse(&mut h, crossterm::event::MouseEventKind::ScrollDown, 75, 10);
    assert_eq!(h.app.detail_scroll, offset);
    assert!(h.app.picker.is_some(), "{picker}");
    h.press(KeyCode::Esc);
    h.press(KeyCode::Char('/'));
    h.press(KeyCode::BackTab);
    assert_eq!(h.app.mode, switchbard_tui::app::Mode::Filter);
    h.press(KeyCode::Esc);
    h.press(KeyCode::Char('?'));
    h.press(KeyCode::PageDown);
    assert_eq!(h.app.pane, switchbard_tui::app::Pane::Help);
    assert!(h.app.help_scroll > 0);
}

#[test]
fn focus_binding_is_configurable_and_help_discloses_it() {
    let mut h = long_detail();
    std::fs::write(&h.config_path, "return { keys = { x = 'focus_pane' } }").unwrap();
    h.app.tick();
    h.press(KeyCode::Enter);
    let list = h.press(KeyCode::Char('x'));
    assert!(list.contains("List active"), "{list}");
    let help = h.press(KeyCode::Char('?'));
    assert!(help.contains("focus_pane"), "{help}");
    assert!(help.contains("shift-tab"), "{help}");
}

#[test]
fn list_wheel_navigates_with_details_open_or_closed() {
    let mut h = Harness::new();
    let first = h.selected_title();
    mouse(&mut h, crossterm::event::MouseEventKind::ScrollDown, 10, 10);
    assert_ne!(h.selected_title(), first);
    h.press(KeyCode::Enter);
    let selected = h.selected_title();
    mouse(&mut h, crossterm::event::MouseEventKind::ScrollDown, 10, 10);
    assert_ne!(h.selected_title(), selected);
    assert!(!h.app.detail_focused());
    assert!(h.render().contains(" Detail "));
}

#[test]
fn empty_and_short_details_clamp_and_page_switch_closes_focus() {
    let mut h = Harness::new();
    h.press(KeyCode::Enter);
    h.press(KeyCode::End);
    assert_eq!(h.app.detail_scroll, 0);
    h.next_list_page();
    assert!(!h.app.detail_focused());
    assert_eq!(h.app.pane, switchbard_tui::app::Pane::None);
    h.next_list_page();
    h.press(KeyCode::Char('/'));
    h.type_text("no-such-task-ever");
    h.press(KeyCode::Enter);
    let empty = h.press(KeyCode::Enter);
    assert!(empty.contains("nothing selected"), "{empty}");
    h.press(KeyCode::End);
    h.press(KeyCode::Down);
    assert_eq!(h.app.detail_scroll, 0);
    h.app.handle_key(crossterm::event::KeyEvent::new(
        KeyCode::Tab,
        crossterm::event::KeyModifiers::SHIFT,
    ));
    assert!(h.render().contains("List active"));
}

#[test]
fn reading_editing_and_pointer_focus_compose_without_losing_the_task() {
    use crossterm::event::{MouseButton, MouseEventKind};
    use switchbard_tui::app::{Mode, Pane};
    let mut h = long_detail();
    let selected = h.selected_title();
    h.press(KeyCode::Enter);
    h.press(KeyCode::PageDown);
    assert!(h.app.detail_scroll > 0);
    h.press(KeyCode::Enter);
    assert_eq!(h.app.mode, Mode::DetailFocus);
    assert!(h.render().contains("enter/l edit"));
    h.press(KeyCode::Esc);
    assert_eq!(h.app.mode, Mode::Browse);
    assert!(!h.app.detail_focused());
    h.press(KeyCode::BackTab);
    assert!(h.app.detail_focused());
    h.press(KeyCode::PageDown);
    let offset = h.app.detail_scroll;
    mouse(&mut h, MouseEventKind::Down(MouseButton::Left), 10, 10);
    assert!(!h.app.detail_focused());
    assert_eq!(h.app.detail_scroll, offset);
    assert_eq!(h.selected_title(), selected);
    h.press(KeyCode::Tab);
    assert_eq!(h.app.pane, Pane::None);
    assert!(!h.app.detail_focused());
}
