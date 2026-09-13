//! Focused navigation and pointer routing for the shared list/detail split.
use super::{App, Mode, Pane};
use crate::{config::Action, page::Page};
use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Position;

fn scrolled(action: &Action, scroll: u16, page: u16) -> Option<u16> {
    Some(match action {
        Action::Down => scroll.saturating_add(1),
        Action::Up => scroll.saturating_sub(1),
        Action::PageDown => scroll.saturating_add(page),
        Action::PageUp => scroll.saturating_sub(page),
        Action::Top => 0,
        Action::Bottom => u16::MAX,
        _ => return None,
    })
}

impl App {
    pub(super) fn apply_detail_action(&mut self, action: &Action) -> bool {
        if self.pane != Pane::Detail {
            return false;
        }
        if *action == Action::FocusPane {
            self.detail.focused = !self.detail.focused;
            self.status.clear();
            return true;
        }
        let pr_page_scroll =
            self.page == Page::PullRequests && matches!(action, Action::PageDown | Action::PageUp);
        if !self.detail.focused && !pr_page_scroll {
            return false;
        }
        let page = self.detail.detail_area.height.saturating_sub(2).max(1);
        let scroll = if self.page == Page::PullRequests {
            &mut self.pull_requests.detail_scroll
        } else {
            &mut self.detail.scroll
        };
        let Some(next) = scrolled(action, *scroll, page) else {
            return false;
        };
        *scroll = next;
        true
    }

    pub fn handle_mouse(&mut self, event: MouseEvent) {
        if self.mode != Mode::Browse || self.picker.is_some() || self.page == Page::Inbox {
            return;
        }
        let action = match event.kind {
            MouseEventKind::ScrollDown => Some(Action::Down),
            MouseEventKind::ScrollUp => Some(Action::Up),
            MouseEventKind::Down(MouseButton::Left) => None,
            _ => return,
        };
        if !self.focus_mouse_pane(Position::new(event.column, event.row)) {
            return;
        }
        if let Some(action) = action {
            self.apply(&action);
            self.telemetry
                .record("action", format!("mouse {}", action.name()));
        }
    }

    fn focus_mouse_pane(&mut self, position: Position) -> bool {
        if self.pane == Pane::Help {
            return self.detail.list_area.contains(position);
        }
        if self.pane == Pane::Detail && self.detail.detail_area.contains(position) {
            self.detail.focused = true;
        } else if self.detail.list_area.contains(position) {
            self.detail.focused = false;
        } else {
            return false;
        }
        true
    }
}
