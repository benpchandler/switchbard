//! Cross-thread coordination primitives. Both types are tiny `Arc<Mutex<…>>`
//! wrappers — the value of having them here is that the rest of the codebase
//! never repeats the condvar dance or the lock/clone/set pattern.

pub mod kick;
pub mod progress;
pub mod status;

pub use kick::Kick;
pub use progress::{BulkProgress, Progress};
pub use status::Status;
