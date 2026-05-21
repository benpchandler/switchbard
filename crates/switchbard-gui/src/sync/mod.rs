//! Cross-thread coordination primitives. Both types are tiny `Arc<Mutex<…>>`
//! wrappers — the value of having them here is that the rest of the codebase
//! never repeats the condvar dance or the lock/clone/set pattern.

pub mod kick;
pub mod status;

pub use kick::Kick;
pub use status::Status;
