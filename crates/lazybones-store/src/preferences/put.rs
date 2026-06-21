//! Upsert the single global user-preferences record.

use surrealdb::Surreal;
use surrealdb::engine::local::Db;

use crate::error::{Result, StoreError};

use super::model::Preferences;
use super::row::{PREFERENCES_KEY, PreferencesRow, SETTINGS_TABLE};

/// Write `prefs`, returning them as stored. Idempotent: writing again
/// overwrites the single record in place.
///
/// # Errors
/// Returns [`StoreError::Operation`] if the write fails.
pub async fn put_preferences(db: &Surreal<Db>, prefs: &Preferences) -> Result<Preferences> {
    let written: Option<PreferencesRow> = db
        .upsert((SETTINGS_TABLE, PREFERENCES_KEY))
        .content(PreferencesRow::from_prefs(prefs))
        .await
        .map_err(StoreError::Operation)?;

    written.map(PreferencesRow::into_prefs).ok_or_else(|| {
        StoreError::Operation(surrealdb::Error::thrown(
            "preferences vanished after write".to_owned(),
        ))
    })
}
