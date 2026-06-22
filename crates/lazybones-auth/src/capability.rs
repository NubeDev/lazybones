//! The capabilities a scoped session may exercise over the REST surface.
//!
//! lazybones runs locally and trusts its loop, but an agent session is handed a
//! scoped grant rather than blanket access: it can drive its own task's lifecycle
//! and write memory, not reconfigure the run. Capabilities are the unit the API
//! checks before a mutating route runs (SCOPE.md, "Scoped session + capability
//! grants").

/// The permission profile a management-agent session runs under. Kept here (not
/// in the store) so the auth crate owns the capability mapping without a
/// dependency cycle; the store's `PermissionProfile` projects into this.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ManagementProfile {
    /// GET-only: explain state, never mutate.
    ReadOnly,
    /// + create/edit workflows, tasks, templates, skills.
    Author,
    /// + propose lifecycle actions (each still confirmed in the UI).
    AuthorAndManage,
}

/// A single thing a session is allowed to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Capability {
    /// Import a workfile (loop only).
    Sync,
    /// Claim a ready task into running.
    Claim,
    /// Heartbeat a running task.
    Heartbeat,
    /// Mark a gating task done.
    Done,
    /// Block a task.
    Block,
    /// Write a memory record.
    Memorize,
    /// Read tasks, runs, and memory.
    Read,
    /// Manage agent CLI credentials (store/list/delete/export). Loop only — an
    /// agent session never reads or writes the secret store.
    Secret,
    /// Create, edit, or delete task records directly (author the queue in the DB; loop only).
    Author,
    /// Author documents, assets, branding, and publish a document to GitHub
    /// (branch/commit/PR/issue). Distinct from [`Author`](Self::Author) (which
    /// means "edit task records"): a document/asset/branding mutation — and
    /// publishing a document — is a *document* action, not a queue action.
    Document,
}

impl Capability {
    /// The full set the trusted loop holds.
    #[must_use]
    pub fn loop_grant() -> &'static [Capability] {
        &[
            Capability::Sync,
            Capability::Claim,
            Capability::Heartbeat,
            Capability::Done,
            Capability::Block,
            Capability::Memorize,
            Capability::Read,
            Capability::Secret,
            Capability::Author,
            Capability::Document,
        ]
    }

    /// The capabilities a management-agent session holds for a permission
    /// profile. The management agent authors and reads through the same REST
    /// surface an operator uses, so its grant is a strict subset of the loop's:
    ///
    /// - `ReadOnly`        → `[Read]`: explain state, never mutate.
    /// - `Author`          → `[Read, Author, Document]`: + create/edit workflows,
    ///   tasks, templates, skills, attachments, and documents/assets/branding (incl.
    ///   publishing a document to GitHub).
    /// - `AuthorAndManage` → `[Read, Author, Document, Block]`: + *propose* lifecycle
    ///   actions. The `Block` grant here is a convenience for reading blocker
    ///   state, **not** a licence to silently start/stop/retry: gated actions are
    ///   emitted as confirm requests and issued by the UI under the operator's
    ///   own token, never the agent's (`docs/agent/lazybones-agent-scope.md` §10.2).
    ///
    /// `Claim` and `Secret` are **never** granted (it does not run the scheduler
    /// loop or read decrypted secrets), and no profile carries a direct
    /// start/stop/retry/delete capability — authoring is not running (§9, §10).
    #[must_use]
    pub fn management_grant(profile: ManagementProfile) -> &'static [Capability] {
        match profile {
            ManagementProfile::ReadOnly => &[Capability::Read],
            ManagementProfile::Author => {
                &[Capability::Read, Capability::Author, Capability::Document]
            }
            ManagementProfile::AuthorAndManage => &[
                Capability::Read,
                Capability::Author,
                Capability::Document,
                Capability::Block,
            ],
        }
    }

    /// The set an agent session holds: drive its task + remember, never `Sync`.
    #[must_use]
    pub fn agent_grant() -> &'static [Capability] {
        &[
            Capability::Heartbeat,
            Capability::Done,
            Capability::Block,
            Capability::Memorize,
            Capability::Read,
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn has(profile: ManagementProfile, cap: Capability) -> bool {
        Capability::management_grant(profile).contains(&cap)
    }

    #[test]
    fn management_grants_widen_by_profile() {
        assert_eq!(
            Capability::management_grant(ManagementProfile::ReadOnly),
            &[Capability::Read]
        );
        assert!(has(ManagementProfile::Author, Capability::Author));
        assert!(has(ManagementProfile::Author, Capability::Document));
        assert!(has(ManagementProfile::AuthorAndManage, Capability::Document));
        assert!(has(ManagementProfile::AuthorAndManage, Capability::Block));
    }

    #[test]
    fn no_management_profile_ever_grants_claim_or_secret() {
        for profile in [
            ManagementProfile::ReadOnly,
            ManagementProfile::Author,
            ManagementProfile::AuthorAndManage,
        ] {
            assert!(!has(profile, Capability::Claim), "{profile:?} must not claim");
            assert!(!has(profile, Capability::Secret), "{profile:?} must not read secrets");
        }
    }
}
