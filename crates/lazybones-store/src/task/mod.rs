//! Task documents: the lifecycle model, its persisted row, and the verbs that
//! read, list, upsert, relate, and transition them.

mod create;
mod delete;
mod depend;
mod get;
mod heartbeat;
mod list;
mod model;
mod row;
mod status;
mod transition;
mod update;
mod upsert;

pub use create::create_task;
pub use delete::delete_task;
pub use depend::{newly_ready, relate_dep, unrelate_dep};
pub use get::get_task;
pub use heartbeat::record_heartbeat;
pub use list::list_tasks;
pub use model::{Task, WorktreeMode};
pub use status::Status;
pub use transition::{Transition, transition_task};
pub use update::{TaskEdit, update_task};
pub use upsert::upsert_task;
