//! IA V2 (TASK-77 decision record) **place** bodies — the real per-place
//! surfaces the sidebar's routing map (`ui::nav`) eventually points every
//! `Place` variant at, replacing the pre-IA-V2 lens code one place at a
//! time. `HiveApp::render_ui` (`app.rs`) is the only caller; nothing here
//! renders navigation itself (that's `ui::nav`'s job).
//!
//! Each place is its own module, built by its own implementation task
//! (TASK-96..101) and landing independently — a place module here does not
//! imply every `Place` variant has one yet; unmigrated places keep routing
//! to their pre-existing `ui::backlog`/`ui::workspace`/`ui::agents` body
//! until their own task lands.

pub(crate) mod goals;
