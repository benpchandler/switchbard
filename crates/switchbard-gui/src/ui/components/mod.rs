//! Reusable UI components — the design-system layer.
//!
//! Every view assembles its tables out of these primitives instead of
//! hand-rolling `RichText::new(...).small().weak()` chains. The contract:
//!
//! - **One source of truth per visual concept.** A "weak dash placeholder"
//!   means exactly the same thing in every view — so it lives here.
//! - **Tokens come from `ui::theme`.** Components consume colors and font
//!   helpers from theme; they never pick a hex value directly.
//! - **No view-specific data shape.** A component takes the primitive it
//!   renders (a `&Path`, an `Option<u64>`, a `&str`) and nothing else. Views
//!   massage their domain data into those primitives.
//!
//! Adding a new visual concept: add a module here, re-export it below, and
//! migrate the inline call sites. Don't extend an existing view file with a
//! new free function — that's the duplication trap we're solving.

pub mod badge;
pub mod branch_label;
pub mod mono_cell;
pub mod path_cell;
pub mod section;
pub mod status_pill;
pub mod strings;
pub mod table_shell;

pub use badge::{count_badge, weak_dash, weak_dots};
pub use branch_label::branch_label;
pub use mono_cell::{mono_label, short_sha};
pub use path_cell::path_cell;
pub use section::{repo_section_header, repo_section_separator, Chip};
pub use status_pill::{status_pill, StatusKind};
pub use table_shell::table_shell;
