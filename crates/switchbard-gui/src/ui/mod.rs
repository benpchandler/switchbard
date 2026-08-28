//! Everything that talks to egui. `theme` is the single source for all
//! semantic colors and glyph constants the views consume. `workspace` is
//! the central panel — per-repo swimlane cards with smart progressive
//! disclosure (worktree rows auto-expand when noteworthy).

pub mod agent_context;
pub mod agent_hooks;
pub mod agents;
pub mod backlog;
pub mod column_widths;
pub mod components;
pub mod dispatch;
pub mod filter_bar;
pub mod legibility;
pub mod onboarding;
pub mod path_display;
pub mod settings;
pub mod sidebar;
pub mod theme;
pub mod top_bar;
pub mod workspace;
