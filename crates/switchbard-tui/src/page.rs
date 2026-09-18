//! Top-level page identity and the actions available on each page.
use crate::config::Action;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Page {
    #[default]
    Tasks,
    PullRequests,
    /// The live agent sessions in this repo (TASK-225).
    Agents,
    Inbox,
}

impl Page {
    pub fn toggle(self) -> Self {
        match self {
            Self::Tasks => Self::PullRequests,
            Self::PullRequests => Self::Agents,
            Self::Agents => Self::Inbox,
            Self::Inbox => Self::Tasks,
        }
    }

    /// Pages whose list is a task or PR view: filters, columns, sort and
    /// paint act on these and nowhere else.
    pub fn has_list_view(self) -> bool {
        matches!(self, Self::Tasks | Self::PullRequests)
    }

    /// Pages with a cursor to move and a row to open.
    pub fn has_cursor(self) -> bool {
        matches!(
            self,
            Self::Tasks | Self::PullRequests | Self::Agents | Self::Inbox
        )
    }

    pub fn allows(self, action: &Action) -> bool {
        action.available_on(self)
    }
}
