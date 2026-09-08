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

#[derive(Clone, Copy)]
enum Availability {
    Tasks,
    Lists,
    Everywhere,
}

// Ordered as displayed in help: action, canonical Lua name, page availability.
const ACTIONS: &[(Action, &str, Availability)] = &[
    (Action::Page, "page", Availability::Everywhere),
    (Action::NewTask, "new_task", Availability::Tasks),
    (Action::Down, "down", Availability::Lists),
    (Action::Up, "up", Availability::Lists),
    (Action::Top, "top", Availability::Lists),
    (Action::Bottom, "bottom", Availability::Lists),
    (Action::PageDown, "page_down", Availability::Lists),
    (Action::PageUp, "page_up", Availability::Lists),
    (Action::Open, "open", Availability::Lists),
    (Action::Back, "back", Availability::Everywhere),
    (Action::Filter, "filter", Availability::Lists),
    (Action::FilterColumn, "filter_column", Availability::Lists),
    (Action::SortColumn, "sort_column", Availability::Lists),
    (Action::Columns, "columns", Availability::Lists),
    (Action::Paint, "paint", Availability::Lists),
    (Action::Ball, "ball", Availability::Tasks),
    (Action::Pass, "pass", Availability::Tasks),
    (Action::Group, "group", Availability::Tasks),
    (Action::Settings, "settings", Availability::Tasks),
    (Action::Rank, "task", Availability::Tasks),
    (Action::Command, "command", Availability::Everywhere),
    (Action::Reload, "reload", Availability::Everywhere),
    (Action::OpenBrowser, "open_browser", Availability::Lists),
    (Action::Merge, "merge", Availability::Lists),
    (
        Action::DismissNotifications,
        "dismiss_notifications",
        Availability::Everywhere,
    ),
    (Action::Help, "help", Availability::Everywhere),
    (Action::View, "view", Availability::Lists),
    (Action::Quit, "quit", Availability::Everywhere),
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

    fn metadata(&self) -> (&'static str, Availability) {
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
        match self.metadata().1 {
            Availability::Tasks => page == Page::Tasks,
            Availability::Lists => page != Page::Inbox,
            Availability::Everywhere => true,
        }
    }
}
