//! The action catalog shared by Lua configuration, help, telemetry and page gating.
use crate::page::Page;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Down,
    Up,
    Top,
    Bottom,
    PageDown,
    PageUp,
    Open,
    OpenBrowser,
    Merge,
    DismissNotifications,
    NewTask,
    Back,
    Filter,
    FilterColumn,
    SortColumn,
    Columns,
    Paint,
    Ball,
    /// Release every session's claim on the selected task: the owner's word.
    Pass,
    Command,
    Reload,
    Help,
    Quit,
    View,
    Page,
    Group,
    Settings,
    /// The task chord: rank digits, Ball, top-list, status, and goals actions.
    Rank,
}

// Ordered as displayed in help: action, canonical Lua name, available on PRs.
const ACTIONS: &[(Action, &str, bool)] = &[
    (Action::Page, "page", true),
    (Action::NewTask, "new_task", false),
    (Action::Down, "down", true),
    (Action::Up, "up", true),
    (Action::Top, "top", true),
    (Action::Bottom, "bottom", true),
    (Action::PageDown, "page_down", true),
    (Action::PageUp, "page_up", true),
    (Action::Open, "open", true),
    (Action::Back, "back", true),
    (Action::Filter, "filter", true),
    (Action::FilterColumn, "filter_column", true),
    (Action::SortColumn, "sort_column", true),
    (Action::Columns, "columns", true),
    (Action::Paint, "paint", true),
    (Action::Ball, "ball", false),
    (Action::Pass, "pass", false),
    (Action::Group, "group", false),
    (Action::Settings, "settings", false),
    (Action::Rank, "task", false),
    (Action::Command, "command", true),
    (Action::Reload, "reload", true),
    (Action::OpenBrowser, "open_browser", true),
    (Action::Merge, "merge", true),
    (Action::DismissNotifications, "dismiss_notifications", true),
    (Action::Help, "help", true),
    (Action::View, "view", true),
    (Action::Quit, "quit", true),
];

impl Action {
    pub fn all() -> impl Iterator<Item = Self> {
        ACTIONS.iter().map(|(action, _, _)| *action)
    }

    pub(crate) fn parse(text: &str) -> Option<Self> {
        let canonical = if text == "rank" { "task" } else { text };
        ACTIONS
            .iter()
            .find(|(_, name, _)| *name == canonical)
            .map(|(action, _, _)| *action)
    }

    fn metadata(&self) -> (&'static str, bool) {
        let (_, name, pull_requests) = ACTIONS
            .iter()
            .find(|(action, _, _)| action == self)
            .expect("every Action has one catalog entry");
        (name, *pull_requests)
    }

    pub fn name(&self) -> String {
        self.metadata().0.to_string()
    }

    pub(crate) fn available_on(&self, page: Page) -> bool {
        page == Page::Tasks || self.metadata().1
    }
}
