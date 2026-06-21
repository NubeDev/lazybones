//! `GET /settings/preferences` + `PUT /settings/preferences` — the single
//! global user-preferences record (display timezone, UI theme).
//!
//! These are operator UI choices that should follow the operator across
//! browsers/devices, rather than living in each browser's `localStorage`.
//! Reads return the stored preferences (or usable defaults when unset). Writes
//! require `Author`.

use axum::Json;
use axum::extract::State;
use lazybones_auth::Capability;
use lazybones_store::Preferences;

use crate::dto::PreferencesBody;
use crate::error::ApiResult;
use crate::extract::Session;
use crate::state::AppState;

/// `GET /settings/preferences` — the current preferences, or the defaults if the
/// operator has never saved any. Open read (local single-user daemon).
pub async fn get_preferences(
    State(state): State<AppState>,
) -> ApiResult<Json<Preferences>> {
    let prefs = state.store.get_preferences().await?.unwrap_or_default();
    Ok(Json(prefs))
}

/// `PUT /settings/preferences` — replace the preferences record. Requires
/// `Author`. An omitted field clears that preference (reverts to its default).
pub async fn put_preferences(
    State(state): State<AppState>,
    session: Session,
    Json(body): Json<PreferencesBody>,
) -> ApiResult<Json<Preferences>> {
    session.require(Capability::Author, "author", "preferences")?;

    // An empty timezone string means "follow the browser" — normalise to None.
    let timezone = body
        .timezone
        .map(|tz| tz.trim().to_owned())
        .filter(|tz| !tz.is_empty());

    let prefs = Preferences {
        timezone,
        theme: body.theme,
        updated_at: state.store.now(),
    };
    Ok(Json(state.store.put_preferences(&prefs).await?))
}
