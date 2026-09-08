//! Frontend-local list settings scope. Page is the list identity; the column
//! catalog remains the authority for individual column properties.
use std::path::{Path, PathBuf};

use crate::{columns::Column, page::Page, views::ViewState};

#[derive(Clone, Copy)]
pub struct ListSettings(pub Page);

impl ListSettings {
    pub fn catalog(self) -> &'static [Column] {
        match self.0 {
            Page::Tasks => &Column::ALL,
            Page::PullRequests => &Column::PR_ALL,
        }
    }

    pub fn default_columns(self) -> &'static [Column] {
        match self.0 {
            Page::Tasks => &Column::DEFAULT_SHOWN,
            Page::PullRequests => &Column::PR_DEFAULT,
        }
    }

    pub fn canonical(self, column: Column) -> Column {
        match (self.0, column) {
            (Page::PullRequests, Column::Status) => Column::Lifecycle,
            _ => column,
        }
    }

    pub fn supports_grouping(self) -> bool {
        self.0 == Page::Tasks
    }
    pub fn supports_top_list(self) -> bool {
        self.0 == Page::Tasks
    }
    pub fn supports_abbreviation(self) -> bool {
        self.0 == Page::Tasks
    }

    pub fn path(self, base: &Path) -> PathBuf {
        match self.0 {
            Page::Tasks => base.to_path_buf(),
            Page::PullRequests => base.with_extension("prs.lua"),
        }
    }

    pub fn defaults(self) -> Vec<ViewState> {
        match self.0 {
            Page::Tasks => crate::views::starter_views(),
            Page::PullRequests => vec![ViewState::pull_requests()],
        }
    }
}
