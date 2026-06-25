//! The user-preferences model — a single global record of operator UI choices.
//!
//! One preferences record per install (single-user daemon). It holds the
//! display/UX choices the operator makes in the settings page that should
//! follow them across browsers and devices, rather than living in each
//! browser's `localStorage`. The timezone is the first such field; the struct
//! is laid out so new preferences can be added as optional columns without a
//! migration.

use serde::{Deserialize, Serialize};

/// Operator config for **content sync** — the git-backed sync repo that carries
/// the export tree (`crate::export_all`) between machines. Lives in preferences
/// because, like timezone/theme, it's a per-operator choice that should follow
/// them; it's null until the operator sets it up in the settings UI.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct SyncConfig {
    /// Master switch for the **automatic** behaviours (boot auto-pull + periodic
    /// auto-push). When `false`, sync still works — it's just manual (the
    /// pull/push buttons). Manual actions are gated only on a configured remote,
    /// never on this flag.
    #[serde(default)]
    pub enabled: bool,
    /// The git remote URL of the cloud sync repo (e.g.
    /// `git@github.com:me/lazybones-sync.git`). `None` => sync not configured.
    #[serde(default)]
    pub remote: Option<String>,
    /// The branch to sync on. `None` is treated as `"main"`.
    #[serde(default)]
    pub branch: Option<String>,
    /// Absolute path to the local checkout. `None` lets the daemon derive one
    /// under its data dir.
    #[serde(default)]
    pub dir: Option<String>,
    /// Push the export automatically after store changes ("before you leave").
    #[serde(default)]
    pub auto_push: bool,
    /// Pull + import automatically on daemon boot ("catch up on PC-2").
    #[serde(default)]
    pub auto_pull: bool,
}

impl SyncConfig {
    /// The effective branch, defaulting to `main` when unset.
    #[must_use]
    pub fn branch_or_default(&self) -> &str {
        self.branch.as_deref().filter(|b| !b.is_empty()).unwrap_or("main")
    }

    /// Whether a usable remote is configured (the gate on every sync action).
    #[must_use]
    pub fn is_configured(&self) -> bool {
        self.remote.as_deref().is_some_and(|r| !r.trim().is_empty())
    }

    /// Whether the daemon should auto-pull on boot: a configured remote, the
    /// master switch on, and the per-behaviour flag set.
    #[must_use]
    pub fn auto_pull_active(&self) -> bool {
        self.enabled && self.auto_pull && self.is_configured()
    }

    /// Whether the daemon should auto-push periodically: a configured remote, the
    /// master switch on, and the per-behaviour flag set.
    #[must_use]
    pub fn auto_push_active(&self) -> bool {
        self.enabled && self.auto_push && self.is_configured()
    }
}

/// The single global user-preferences record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Preferences {
    /// IANA timezone name all dates/times are shown in (e.g.
    /// `"Asia/Ho_Chi_Minh"`). `None` (or empty) means "follow the browser".
    pub timezone: Option<String>,
    /// UI theme: `"light" | "dark" | "system"`. `None` means "system".
    pub theme: Option<String>,
    /// Content-sync configuration; `None` until the operator sets it up.
    #[serde(default)]
    pub sync: Option<SyncConfig>,
    /// RFC3339 timestamp of the last write.
    pub updated_at: String,
}

impl Default for Preferences {
    /// The defaults the API returns when the operator has never saved any: no
    /// timezone override (follow the browser), the system theme, and no sync.
    fn default() -> Self {
        Self {
            timezone: None,
            theme: None,
            sync: None,
            updated_at: String::new(),
        }
    }
}
