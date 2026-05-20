//! Everything that talks to egui. Each view module owns one central panel
//! (plus, for top_bar, the top panel). `theme` is the single source for all
//! semantic colors and glyph constants the views consume.

pub mod listeners;
pub mod servers;
pub mod sidebar;
pub mod theme;
pub mod top_bar;
pub mod worktrees;
