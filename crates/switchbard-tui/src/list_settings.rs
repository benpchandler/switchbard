//! Frontend-local list settings scope. Only list-bearing pages have a scope; the column
//! catalog remains the authority for individual column properties.
use std::path::{Path, PathBuf};

use crate::{
    columns::{Column, ColumnRegistry},
    page::Page,
    views::ViewState,
};

#[derive(Clone, Copy)]
pub enum ListSettings {
    Tasks,
    PullRequests,
}

impl ListSettings {
    pub fn for_page(page: Page) -> Option<Self> {
        match page {
            Page::Tasks => Some(Self::Tasks),
            Page::PullRequests => Some(Self::PullRequests),
            Page::Inbox => None,
        }
    }

    /// Every column this page can show. The task page's catalog grows with the
    /// repo's own declared fields, which is why it is built rather than fixed.
    pub fn catalog(self, registry: &ColumnRegistry) -> Vec<Column> {
        match self {
            Self::Tasks => registry.task_columns(),
            Self::PullRequests => Column::PR_ALL.to_vec(),
        }
    }

    pub fn default_columns(self) -> &'static [Column] {
        match self {
            Self::Tasks => &Column::DEFAULT_SHOWN,
            Self::PullRequests => &Column::PR_DEFAULT,
        }
    }

    pub fn canonical(self, column: Column) -> Column {
        match (self, column) {
            (Self::PullRequests, Column::Status) => Column::Lifecycle,
            _ => column,
        }
    }

    pub fn supports_grouping(self) -> bool {
        matches!(self, Self::Tasks)
    }
    pub fn supports_top_list(self) -> bool {
        matches!(self, Self::Tasks)
    }
    pub fn supports_abbreviation(self) -> bool {
        matches!(self, Self::Tasks)
    }

    pub fn path(self, base: &Path) -> PathBuf {
        match self {
            Self::Tasks => base.to_path_buf(),
            Self::PullRequests => base.with_extension("prs.lua"),
        }
    }

    pub fn defaults(self) -> Vec<ViewState> {
        match self {
            Self::Tasks => crate::views::starter_views(),
            Self::PullRequests => vec![ViewState::pull_requests()],
        }
    }
}
