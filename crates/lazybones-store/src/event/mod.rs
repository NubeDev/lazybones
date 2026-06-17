//! Run-log events: the structured, queryable replacement for an appended loop log.

mod append;
mod history;
mod row;

pub use append::append_event;
pub use history::run_history;
pub use row::Event;
