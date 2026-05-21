//! Internal modules for the Switchbard GUI binary. Kept as a library crate so each
//! module compiles in isolation and integration tests can exercise the
//! domain types without going through eframe.
//!
//! Layout:
//! - `ui/`      — anything that renders to egui (theme + the four views).
//! - `runtime/` — plain-data domain types + the worktree-expansion helper.
//! - `sync/`    — cross-thread coordination primitives (Kick, Status).
//! - `app`      — `HiveApp`: ties everything together.
//! - `workers`  — background threads that feed the GUI.

pub mod app;
pub mod runtime;
pub mod sync;
pub mod ui;
pub mod workers;
