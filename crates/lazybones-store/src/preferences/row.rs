//! The persisted shape of [`Preferences`] — one fixed row.
//!
//! A single global record lives at `settings:preferences` (a settings-style row
//! keyed by a constant, sharing the `settings` table with the management-agent
//! config). Every column is optional so new preferences land without a migration.

use surrealdb::types::{RecordId, SurrealValue};

use super::model::Preferences;

/// The settings table the preferences record lives in.
pub(crate) const SETTINGS_TABLE: &str = "settings";

/// The fixed record key for the single global preferences row.
pub(crate) const PREFERENCES_KEY: &str = "preferences";

/// SurrealDB-facing preferences: the reserved `id` thing plus the fields.
#[derive(Debug, Clone, PartialEq, SurrealValue)]
pub(crate) struct PreferencesRow {
    pub(crate) id: RecordId,
    pub(crate) timezone: Option<String>,
    pub(crate) theme: Option<String>,
    pub(crate) updated_at: Option<String>,
}

impl PreferencesRow {
    /// Project domain [`Preferences`] into its persisted row.
    pub(crate) fn from_prefs(p: &Preferences) -> Self {
        Self {
            id: RecordId::new(SETTINGS_TABLE, PREFERENCES_KEY),
            timezone: p.timezone.clone(),
            theme: p.theme.clone(),
            updated_at: Some(p.updated_at.clone()),
        }
    }

    /// Reconstruct the domain [`Preferences`].
    pub(crate) fn into_prefs(self) -> Preferences {
        Preferences {
            timezone: self.timezone,
            theme: self.theme,
            updated_at: self.updated_at.unwrap_or_default(),
        }
    }
}
