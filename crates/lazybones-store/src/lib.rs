//! Embedded SurrealDB store boundary for lazybones — the durable brain of a run.
//!
//! The single source of truth (SCOPE.md principle 6): task documents, the
//! `depends_on` graph that drives readiness, and the `event` run log. Everything
//! a run needs to survive a restart lives here, reached over the [`StoreHandle`].

mod agent;
mod agent_chat;
mod asset;
mod attachment;
mod bootstrap;
mod branding;
mod chat;
mod check_health;
mod connect;
mod document;
mod error;
mod event;
mod extension;
mod follow_up;
mod handle;
mod hcom_log;
mod init_schema;
mod run;
mod management_agent;
mod preferences;
mod secret;
mod skill;
mod source;
mod task;
mod template;
mod workfile;

pub use agent::{AgentCatalog, AgentCatalogEdit, seed_default_agents};
pub use agent_chat::{AgentConversation, AgentMessage, AgentRole, ConfirmAction};
pub use asset::{Asset, AssetError, BlobStore, FileBlobStore, sha256_hex};
pub use attachment::Attachment;
pub use branding::{BrandColors, BrandFonts, Branding, seed_default_branding};
pub use chat::{ChatMessage, ChatRole};
pub use connect::StoreEngine;
pub use document::{DocKind, DocRepo, Document};
pub use error::{Result, StoreError};
pub use event::{Activity, Event, EventBus, LiveEvent};
pub use extension::{Extension, ExtensionSource, FrontendDescriptor};
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
pub use source::{Source, SourceKind, extract_pdf_text};
pub use task::{
    DEFAULT_MAX_RETRIES, IssueSyncState, RetryStrategy, Status, Task, TaskEdit, Transition,
    WorktreeMode, issue_number_from_url,
};
pub use template::{Template, instantiate};
pub use workfile::{SeedTask, deps_with_reuse, sync_seeds};
