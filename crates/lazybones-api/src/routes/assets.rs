//! `/assets` — the content-addressed file server (logos, images).
//!
//! Bytes are uploaded as a **raw body** with `Content-Type` and `X-Filename`
//! headers (deliberately *not* `multipart`, to keep that axum feature out of the
//! workspace). The sha256 is computed server-side; identical bytes dedup to one
//! [`Asset`](lazybones_store::Asset) row (reusable images for free). The bytes
//! live behind the [`BlobStore`](lazybones_store::BlobStore) in
//! [`AppState::assets`]; this row is metadata only.
//!
//! Mutations require [`Capability::Document`]; reads are open (like `/tasks`), so
//! `GET /assets/:id` doubles as the logo/image `src` the UI points at.

use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use axum::http::header::CONTENT_TYPE;
use axum::response::{IntoResponse, Response};
use lazybones_auth::Capability;
use lazybones_store::{Asset, StoreError, sha256_hex};

use crate::dto::ProjectQuery;
use crate::error::{ApiError, ApiResult};
use crate::extract::Session;
use crate::state::AppState;

/// `GET /assets` — list asset metadata (open read), optionally `?project=`.
pub async fn list_assets(
    State(state): State<AppState>,
    Query(query): Query<ProjectQuery>,
) -> ApiResult<Json<Vec<Asset>>> {
    Ok(Json(state.store.list_assets(query.project.as_deref()).await?))
}

/// `POST /assets` — upload bytes (raw body). Requires `Document`. The
/// `Content-Type` header sets the asset's content type; `X-Filename` its display
/// name. Content-addressed: re-uploading identical bytes returns the existing
/// asset and is a harmless blob overwrite.
pub async fn create_asset(
    State(state): State<AppState>,
    session: Session,
    Query(query): Query<ProjectQuery>,
    headers: HeaderMap,
    body: Bytes,
) -> ApiResult<Json<Asset>> {
    session.require(Capability::Document, "document", "asset")?;
    if body.is_empty() {
        return Err(ApiError::bad_request("asset body must not be empty"));
    }
    let content_type = headers
        .get(CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("application/octet-stream")
        .to_owned();
    let filename = headers
        .get("x-filename")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("upload")
        .to_owned();
    let sha = sha256_hex(&body);
    let project = query.project.clone();

    let asset = Asset {
        id: format!("asset-{}", &sha[..sha.len().min(16)]),
        project: project.clone(),
        filename,
        content_type,
        size: body.len() as u64,
        sha256: sha.clone(),
        created_at: state.store.now(),
    };
    // Metadata first (dedups on sha256+project), then persist the bytes keyed by
    // the *stored* asset's content address — idempotent if the blob already exists.
    let stored = state.store.create_asset(&asset).await?;
    state
        .assets
        .put(&stored.sha256, stored.project.as_deref(), &body)
        .await?;
    Ok(Json(stored))
}

/// `GET /assets/:id` — serve an asset's bytes with its stored content type (open
/// read). This is the file-server endpoint, also used as the logo/image source.
pub async fn get_asset(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<Response> {
    let asset = state
        .store
        .get_asset(&id)
        .await?
        .ok_or(StoreError::AssetNotFound(id))?;
    let bytes = state
        .assets
        .get(&asset.sha256, asset.project.as_deref())
        .await?;
    Ok(([(CONTENT_TYPE, asset.content_type)], bytes).into_response())
}

/// `DELETE /assets/:id` — drop an asset's metadata + bytes. Requires `Document`.
/// Returns whether it existed.
pub async fn delete_asset(
    State(state): State<AppState>,
    session: Session,
    Path(id): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    session.require(Capability::Document, "document", &id)?;
    // Look up the blob address before dropping the row; best-effort delete the
    // bytes (other assets never share a sha here — the id is sha-derived).
    if let Some(asset) = state.store.get_asset(&id).await? {
        let existed = state.store.delete_asset(&id).await?;
        let _ = state
            .assets
            .delete(&asset.sha256, asset.project.as_deref())
            .await?;
        Ok(Json(serde_json::json!({ "deleted": existed })))
    } else {
        Ok(Json(serde_json::json!({ "deleted": false })))
    }
}
