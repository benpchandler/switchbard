//! Bug runs and owner handoffs, separate from canonical task authority.
mod codex;
mod identity;
mod owner_commands;
mod process;
mod publish;
mod runner;
mod store;
mod task_flow;
mod transitions;
mod types;
mod worker_commands;

pub use identity::repository_key;
pub use publish::reconcile_publication;
pub use runner::run_supervisor;
pub use store::{default_path, Store};
pub use types::{
    AgentOutcome, Lease, Message, MessageRole, Run, RunAction, RunOptions, RunState,
    TaskStorageIdentity, Worker, MAX_EVIDENCE, MAX_MESSAGES, MAX_RUNS, MAX_TEXT,
};

#[cfg(test)]
mod tests;

pub use publish::review_diff;
