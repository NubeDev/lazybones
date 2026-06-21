//! Embedded SurrealDB store boundary for lazybones — the durable brain of a run.
//!
//! The single source of truth (SCOPE.md principle 6): task documents, the
//! `depends_on` graph that drives readiness, and the `event` run log. Everything
//! a run needs to survive a restart lives here, reached over the [`StoreHandle`].

mod agent;
mod agent_chat;
mod attachment;
mod bootstrap;
mod chat;
mod check_health;
mod connect;
mod error;
mod event;
mod follow_up;
mod handle;
mod hcom_log;
mod init_schema;
mod run;
mod management_agent;
mod preferences;
mod secret;
mod skill;
mod task;
mod template;
mod workfile;

pub use agent::{AgentCatalog, AgentCatalogEdit, seed_default_agents};
pub use agent_chat::{AgentConversation, AgentMessage, AgentRole, ConfirmAction};
pub use attachment::Attachment;
pub use chat::{ChatMessage, ChatRole};
pub use connect::StoreEngine;
pub use error::{Result, StoreError};
pub use event::{Activity, Event, EventBus, LiveEvent};
pub use follow_up::{FollowUp, FollowUpFilter, NewFollowUpEntry};
pub use handle::StoreHandle;
pub use hcom_log::{HcomLogEntry, HcomLogFilter, NewHcomLogEntry};
pub use management_agent::{
    ManagementAgentConfig, ManagementAgentScope, PermissionProfile, SessionMode,
};
pub use preferences::Preferences;
pub use run::{Lifecycle, MergeMode, Run, RunState, Workspace, derived_state};
pub use secret::{SecretEnv, SecretMeta};
pub use skill::{Skill, SkillAction, SkillParam, seed_default_skills};
pub use task::{
    DEFAULT_MAX_RETRIES, RetryStrategy, Status, Task, TaskEdit, Transition, WorktreeMode,
};
pub use template::{Template, instantiate};
pub use workfile::{SeedTask, deps_with_reuse, sync_seeds};
