//! The scheduler: the execution plane that drives ready tasks to done.

mod auto_pr;
mod block;
mod effective;
pub mod ext;
pub(crate) mod finish;
mod follow_up;
mod gate;
mod gate_preflight;
mod git;
mod hcom_tail;
pub mod issue;
mod merge;
mod preflight;
mod prompt;
mod reclaim;
mod run;
mod tick;
mod worktree;

pub use preflight::workspace_preflight;
pub use run::{run, run_with_ext};

pub(crate) use tick::tick;
