//! The scheduler: the execution plane that drives ready tasks to done.

mod auto_pr;
mod block;
mod effective;
pub(crate) mod finish;
mod follow_up;
mod gate;
mod git;
mod hcom_tail;
pub mod issue;
mod merge;
mod prompt;
mod reclaim;
mod run;
mod tick;
mod worktree;

pub use run::run;

pub(crate) use tick::tick;
