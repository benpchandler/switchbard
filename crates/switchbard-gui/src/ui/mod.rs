//! Everything that talks to egui. `theme` is the single source for all
//! semantic colors and glyph constants the views consume. `places` holds
//! each IA V2 place's own body module — `places::ops` (TASK-100) is the
//! merged Servers/Workspace central panel, one row per worktree.

pub mod agent_context;
pub mod agent_hooks;
pub mod backlog;
pub mod column_widths;
pub mod components;
pub mod dispatch;
pub mod filter_bar;
pub mod legibility;
pub mod missions;
pub mod nav;
pub mod onboarding;
pub mod path_display;
pub mod places;
pub mod settings;
pub mod sidebar;
pub mod tasks_read_state;
pub mod theme;
pub mod top_bar;
