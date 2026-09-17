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
    FocusPane,
    OpenBrowser,
    Merge,
    KillAgent,
    /// Toggle the cursor row's bulk-merge mark (PR page).
    Mark,
    /// Word-processor-style range select: extend the bulk-merge mark from an
    /// anchor to the cursor, one row down at a time (PR page).
    ExtendMarkDown,
    /// The same range select, one row up (PR page).
    ExtendMarkUp,
    DismissNotifications,
    NewTask,
    RepoIdea,
    RepoBug,
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
    Agents,
    Tasks,
    PullRequests,
    /// Task and PR pages: the ones with a filterable, paintable list view.
    Lists,
    /// Every page with a cursor, the Agents page included.
    Cursor,
    Everywhere,
}

impl Availability {
    /// The one sentence naming where an action works, so a page that refuses a
    /// key and the help that offers it can never disagree about it.
    fn where_it_works(self) -> &'static str {
        match self {
            Self::Agents => "the Agents page",
            Self::Tasks => "the Tasks page",
            Self::PullRequests => "the Pull Requests page",
            Self::Lists => "Tasks and Pull Requests",
            Self::Cursor => "any page with a cursor",
            Self::Everywhere => "every page",
        }
    }

    /// Whether this tier means the same thing on every page it appears on.
    /// Only these carry [`LOCKED`] actions.
    fn is_fixed_vocabulary(self) -> bool {
        matches!(self, Self::Cursor | Self::Everywhere)
    }
}

/// The fixed vocabulary: keys whose meaning never changes between pages, so a
/// user's fingers can trust them anywhere. No page may reassign one, every one
/// must declare a [`Availability::is_fixed_vocabulary`] tier, and `default.lua`
/// must bind every one of them. [`missing_locked`] reports any drift.
pub const LOCKED: &[Action] = &[
    Action::Page,
    Action::Back,
    Action::Down,
    Action::Up,
    Action::Top,
    Action::Bottom,
    Action::PageDown,
    Action::PageUp,
    Action::Open,
    Action::Command,
    Action::Reload,
    Action::Help,
    Action::Quit,
];

// Ordered as displayed in help: action, canonical Lua name, page availability.
const ACTIONS: &[(Action, &str, Availability)] = &[
    (Action::Page, "page", Availability::Everywhere),
    (Action::FocusPane, "focus_pane", Availability::Lists),
    (Action::NewTask, "new_task", Availability::Tasks),
    (Action::RepoIdea, "repo_idea", Availability::Everywhere),
    (Action::RepoBug, "repo_bug", Availability::Everywhere),
    (Action::Down, "down", Availability::Cursor),
    (Action::Up, "up", Availability::Cursor),
    (Action::Top, "top", Availability::Cursor),
    (Action::Bottom, "bottom", Availability::Cursor),
    (Action::PageDown, "page_down", Availability::Cursor),
    (Action::PageUp, "page_up", Availability::Cursor),
    (Action::Open, "open", Availability::Cursor),
    (Action::Back, "back", Availability::Everywhere),
    (Action::Filter, "filter", Availability::Lists),
    (Action::FilterColumn, "filter_column", Availability::Lists),
    (Action::SortColumn, "sort_column", Availability::Lists),
    (Action::Columns, "columns", Availability::Lists),
    (Action::Paint, "paint", Availability::Lists),
    (Action::Ball, "ball", Availability::Tasks),
    (Action::Pass, "pass", Availability::Tasks),
    (Action::Group, "outline", Availability::Tasks),
    (Action::Settings, "settings", Availability::Tasks),
    (Action::Rank, "task", Availability::Tasks),
    (Action::Command, "command", Availability::Everywhere),
    (Action::Reload, "reload", Availability::Everywhere),
    (
        Action::OpenBrowser,
        "open_browser",
        Availability::PullRequests,
    ),
    (Action::Merge, "merge", Availability::PullRequests),
    (Action::KillAgent, "kill_agent", Availability::Agents),
    (Action::Mark, "mark", Availability::PullRequests),
    (
        Action::ExtendMarkDown,
        "extend_down",
        Availability::PullRequests,
    ),
    (
        Action::ExtendMarkUp,
        "extend_up",
        Availability::PullRequests,
    ),
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
        // Both aliases keep an old `tui.lua` working after its action was
        // renamed to the word this catalog now displays and teaches (`task`,
        // `outline`); `name()` never reads the alias back.
        let canonical = match text {
            "rank" => "task",
            "group" => "outline",
            other => other,
        };
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
            Availability::Agents => page == Page::Agents,
            Availability::Tasks => page == Page::Tasks,
            Availability::PullRequests => page == Page::PullRequests,
            Availability::Lists => page.has_list_view(),
            Availability::Cursor => page.has_cursor(),
            Availability::Everywhere => true,
        }
    }

    /// What to tell the user who pressed this key on a page that refuses it.
    pub(crate) fn where_it_works(&self) -> &'static str {
        self.metadata().1.where_it_works()
    }
}

/// Every locked action that `bound` never binds, plus every one whose catalog
/// tier lets a page take it away. A non-empty result means the fixed vocabulary
/// has drifted: a key the user is entitled to press everywhere has gone missing
/// or become page-local.
pub fn missing_locked(bound: impl Iterator<Item = Action>) -> Vec<String> {
    let bound: Vec<Action> = bound.collect();
    LOCKED
        .iter()
        .filter_map(|action| {
            let (name, availability) = action.metadata();
            if !availability.is_fixed_vocabulary() {
                return Some(format!(
                    "locked action '{name}' is limited to {}; it must work on every page it appears on",
                    availability.where_it_works()
                ));
            }
            (!bound.contains(action))
                .then(|| format!("locked action '{name}' has no key bound; it must stay reachable"))
        })
        .collect()
}
