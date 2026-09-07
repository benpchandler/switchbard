//! Top-level page identity and the actions available on each page.
use crate::config::Action;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Page {
    #[default]
    Tasks,
    PullRequests,
}

impl Page {
    pub fn toggle(self) -> Self {
        match self {
            Self::Tasks => Self::PullRequests,
            Self::PullRequests => Self::Tasks,
        }
    }

    pub fn allows(self, action: &Action) -> bool {
        self == Self::Tasks
            || matches!(
                action,
                Action::Page
                    | Action::Help
                    | Action::Back
                    | Action::Quit
                    | Action::Command
                    | Action::Reload
            )
    }
}
