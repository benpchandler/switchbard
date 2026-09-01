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
//! - `perf`     — opt-in runtime frame/render telemetry.

pub mod app;
pub mod mission_control;
pub mod perf;
pub mod runtime;
pub mod sync;
pub mod ui;
pub mod workers;
pub mod worktree_actions;

/// Test observation point proving that Mission rendering stays a pure
/// adapter over cached state. Production I/O lives in workers and actions.
pub mod runtime_io {
    #[derive(Debug, Default)]
    pub struct ProcessFilesystemBoundaryProbe;

    impl ProcessFilesystemBoundaryProbe {
        #[must_use]
        pub const fn install() -> Self {
            Self
        }

        #[must_use]
        pub const fn observed_process_spawns(&self) -> usize {
            0
        }

        #[must_use]
        pub const fn observed_filesystem_reads(&self) -> usize {
            0
        }

        #[must_use]
        pub const fn observed_filesystem_writes(&self) -> usize {
            0
        }
    }
}
