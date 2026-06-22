//! Read the single global user-preferences record.

use surrealdb::Surreal;
use surrealdb::engine::local::Db;

use crate::error::{Result, StoreError};

use super::model::Preferences;
use super::row::{PREFERENCES_KEY, PreferencesRow, SETTINGS_TABLE};

/// The global preferences record, or `None` if the operator never saved any.
///
/// # Errors
/// Returns [`StoreError::Operation`] if the read fails.
pub async fn get_preferences(db: &Surreal<Db>) -> Result<Option<Preferences>> {
    let row: Option<PreferencesRow> = db
        .select((SETTINGS_TABLE, PREFERENCES_KEY))
        .await
        .map_err(StoreError::Operation)?;
    Ok(row.map(PreferencesRow::into_prefs))
}
