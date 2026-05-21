//! Everything that talks to egui. `theme` is the single source for all
//! semantic colors and glyph constants the views consume. `workspace` is the
//! central panel that hosts the three collapsible sections (Worktrees,
//! Servers, Listeners) — each section renderer lives in its own module so
//! the column structure stays scoped to one file.

pub mod column_widths;
pub mod components;
pub mod listeners;
pub mod path_display;
pub mod servers;
pub mod sidebar;
pub mod theme;
pub mod top_bar;
pub mod workspace;
pub mod worktrees;
