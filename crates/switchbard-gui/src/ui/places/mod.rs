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
//!
//! - `digest` — TASK-99, landed: goal cards, in-flight work, the "needs a
//!   human" attention feed.
//! - `goals` — TASK-101, landed: the real Goals index and goal page.
//! - [`dispatches`] — TASK-98, landed: the Dispatches built-in view under
//!   the Tasks place (task-scoped delivery state: run status, kill/retry/log
//!   per row).
//! - [`command`] — TASK-98, landed: the Command place (agent-scoped fleet
//!   console: dispatch runs + interactive sessions, plus the pre-existing
//!   Context/Hooks sections it inherited from the old top-level Agents
//!   view).
//!
//! Ops/Tasks place bodies are other fireteams' slices; nothing here
//! licenses adding modules for them ahead of that work.

pub mod command;
pub mod digest;
pub mod dispatches;
pub(crate) mod goals;
