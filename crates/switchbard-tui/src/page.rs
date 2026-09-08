//! Top-level page identity and the actions available on each page.
use crate::config::Action;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Page {
    #[default]
    Tasks,
    PullRequests,
    Inbox,
}

impl Page {
    pub fn toggle(self) -> Self {
        match self {
            Self::Tasks => Self::PullRequests,
            Self::PullRequests => Self::Inbox,
            Self::Inbox => Self::Tasks,
        }
    }

    pub fn allows(self, action: &Action) -> bool {
        action.available_on(self)
    }
}
