//! `/documents/:id/sources` — a document's uploads / context material.
//!
//! Sources sit **behind** a document (links, uploaded PDFs/images) and are
//! **never rendered** into the output — that is what distinguishes them from
//! [references](super::documents) (which *are* merged in). A source rides the
//! generic [`attachment`](lazybones_store::Attachment) seam (`thing_kind="source"`)
//! and carries a direct `document` FK the per-document listing reads.
//!
//! `POST` accepts either a **link** (JSON body) or a **file upload** (raw body +
//! `Content-Type`/`X-Filename` headers, like an asset): the bytes go to the blob
//! store under their sha256 (dedup), and a PDF's text is extracted into
//! `extracted_text`. Mutations require [`Capability::Document`]; the list is open.

use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::http::header::CONTENT_TYPE;
use lazybones_auth::Capability;
use lazybones_store::{Asset, Source, extract_pdf_text, sha256_hex};

use crate::dto::LinkSourceBody;
use crate::error::{ApiError, ApiResult};
use crate::extract::Session;
use crate::routes::documents::require_document;
use crate::state::AppState;

/// The owner kind + attachment thing-kind for a source.
const OWNER_KIND: &str = "document";
const SOURCE_KIND: &str = "source";

/// `GET /documents/:id/sources` — list the sources behind a document (open read),
/// newest first. `404` if the document does not exist.
pub async fn list_sources(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<Json<Vec<Source>>> {
    require_document(&state, &id).await?;
    Ok(Json(state.store.list_sources(&id).await?))
}

/// `POST /documents/:id/sources` — add a link (JSON) or upload a file (raw body).
/// Requires `Document`. `404` if the document does not exist.
pub async fn add_source(
    State(state): State<AppState>,
    session: Session,
    Path(id): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> ApiResult<Json<Source>> {
    session.require(Capability::Document, "document", &id)?;
    require_document(&state, &id).await?;

    let content_type = headers
        .get(CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_owned();

    let source = if content_type.starts_with("application/json") {
        // A link source: parse the JSON body.
        let link: LinkSourceBody = serde_json::from_slice(&body)
            .map_err(|e| ApiError::bad_request(format!("invalid link source body: {e}")))?;
        let sid = mint_source_id(&state, &id, &link.url);
        let title = if link.title.trim().is_empty() {
            link.url.clone()
        } else {
            link.title.clone()
        };
        let source = Source::link(&sid, &id, &link.url, &title, state.store.now());
        state.store.create_source(&source).await?
    } else {
        // A file source: store the bytes, create the asset, extract PDF text.
        if body.is_empty() {
            return Err(ApiError::bad_request("source file body must not be empty"));
        }
        let filename = headers
            .get("x-filename")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("upload")
            .to_owned();
        let ct = if content_type.is_empty() {
            "application/octet-stream".to_owned()
        } else {
            content_type.clone()
        };
        let sha = sha256_hex(&body);
        let asset = Asset {
            id: format!("asset-{}", &sha[..sha.len().min(16)]),
            project: None,
            filename: filename.clone(),
            content_type: ct.clone(),
            size: body.len() as u64,
            sha256: sha.clone(),
            created_at: state.store.now(),
        };
        let stored = state.store.create_asset(&asset).await?;
        state
            .assets
            .put(&stored.sha256, stored.project.as_deref(), &body)
            .await?;

        let sid = mint_source_id(&state, &id, &sha);
        let mut source = Source::file(&sid, &id, &stored.id, &filename, &ct, state.store.now());
        if ct == "application/pdf"
            && let Some(text) = extract_pdf_text(&body)
        {
            source = source.with_extracted_text(text);
        }
        state.store.create_source(&source).await?
    };

    // Record the link on the generic attachment seam too (thing_kind="source"),
    // so reverse lookups work uniformly across references and sources.
    state
        .store
        .attach(OWNER_KIND, &id, SOURCE_KIND, &source.id)
        .await?;
    Ok(Json(source))
}

/// `DELETE /documents/:id/sources/:sid` — remove a source. Requires `Document`.
/// `404` if the document does not exist; returns whether the source existed.
pub async fn remove_source(
    State(state): State<AppState>,
    session: Session,
    Path((id, sid)): Path<(String, String)>,
) -> ApiResult<Json<serde_json::Value>> {
    session.require(Capability::Document, "document", &id)?;
    require_document(&state, &id).await?;
    let existed = state.store.delete_source(&sid).await?;
    let _ = state.store.detach(OWNER_KIND, &id, SOURCE_KIND, &sid).await?;
    Ok(Json(serde_json::json!({ "deleted": existed })))
}

/// Mint a stable-ish unique source id from the document, the current time, and a
/// content discriminator (no RNG dependency in the workspace).
fn mint_source_id(state: &AppState, document: &str, detail: &str) -> String {
    let seed = format!("{document}|{}|{detail}", state.store.now());
    let hash = sha256_hex(seed.as_bytes());
    format!("source-{}", &hash[..16])
}
