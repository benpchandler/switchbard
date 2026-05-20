//! Everything that talks to egui. The central panel is now a single
//! progressive-disclosure tree (`unified`); `theme` is the single source for
//! all semantic colors and glyph constants the views consume.

pub mod column_widths;
pub mod components;
pub mod path_display;
pub mod sidebar;
pub mod theme;
pub mod top_bar;
pub mod unified;
