//! `/documents/:id/pages` — the ordered pages (content) of a document/book.
//!
//! A [`Document`](lazybones_store::Document) is a container; its content lives in
//! these [`Page`](lazybones_store::Page) rows, assembled in `position` order at
//! render time (each page is a page-break boundary in the exported PDF). Order is
//! a fractional `position`, so reordering or inserting a page is a single-field
//! update — the client passes an explicit `position` (e.g. the midpoint of two
//! neighbours) to insert, or omits it to append. Mutations require
//! [`Capability::Document`]; the list is open.

use axum::Json;
use axum::extract::{Path, State};
use lazybones_auth::Capability;
use lazybones_store::{Page, StoreError, append_position, sha256_hex};

use crate::dto::{CreatePageBody, UpdatePageBody};
use crate::error::ApiResult;
use crate::extract::Session;
use crate::routes::documents::require_document;
use crate::state::AppState;

/// `GET /documents/:id/pages` — list a document's pages in render order (open
/// read). `404` if the document does not exist.
pub async fn list_pages(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<Json<Vec<Page>>> {
    require_document(&state, &id).await?;
    Ok(Json(state.store.list_pages(&id).await?))
}

/// `POST /documents/:id/pages` — append (or insert) a page. Requires `Document`.
/// `404` if the document does not exist. With no `position`, the page is appended
/// after the current last page.
pub async fn create_page(
    State(state): State<AppState>,
    session: Session,
    Path(id): Path<String>,
    Json(body): Json<CreatePageBody>,
) -> ApiResult<Json<Page>> {
    session.require(Capability::Document, "document", &id)?;
    require_document(&state, &id).await?;

    // Default to appending after the last page when no explicit position is given.
    let position = match body.position {
        Some(p) => p,
        None => {
            let last = state.store.list_pages(&id).await?.last().map(|p| p.position);
            append_position(last)
        }
    };

    let pid = mint_page_id(&state, &id, &body.title);
    let page = Page::new(&pid, &id, &body.title, &body.body, position, state.store.now());
    Ok(Json(state.store.create_page(&page).await?))
}

/// `GET /documents/:id/pages/:pid` — fetch one page (open read), or `404`.
pub async fn get_page(
    State(state): State<AppState>,
    Path((id, pid)): Path<(String, String)>,
) -> ApiResult<Json<Page>> {
    require_document(&state, &id).await?;
    Ok(Json(require_page(&state, &id, &pid).await?))
}

/// `PUT /documents/:id/pages/:pid` — overwrite a page's fields and/or move it.
/// Requires `Document`. `created_at` is preserved; `updated_at` is stamped now.
pub async fn update_page(
    State(state): State<AppState>,
    session: Session,
    Path((id, pid)): Path<(String, String)>,
    Json(body): Json<UpdatePageBody>,
) -> ApiResult<Json<Page>> {
    session.require(Capability::Document, "document", &id)?;
    require_document(&state, &id).await?;
    let existing = require_page(&state, &id, &pid).await?;

    let mut page = Page::new(&pid, &id, &body.title, &body.body, existing.position, state.store.now());
    // Move only when a new position is supplied; otherwise hold its place.
    if let Some(position) = body.position {
        page.position = position;
    }
    Ok(Json(state.store.update_page(&page).await?))
}

/// `DELETE /documents/:id/pages/:pid` — remove a page. Requires `Document`. `404`
/// if the document does not exist; returns whether the page existed.
pub async fn delete_page(
    State(state): State<AppState>,
    session: Session,
    Path((id, pid)): Path<(String, String)>,
) -> ApiResult<Json<serde_json::Value>> {
    session.require(Capability::Document, "document", &id)?;
    require_document(&state, &id).await?;
    let existed = state.store.delete_page(&pid).await?;
    Ok(Json(serde_json::json!({ "deleted": existed })))
}

/// 404 unless the page exists *and* belongs to this document.
async fn require_page(state: &AppState, document: &str, pid: &str) -> ApiResult<Page> {
    Ok(state
        .store
        .get_page(pid)
        .await?
        .filter(|p| p.document == document)
        .ok_or_else(|| StoreError::PageNotFound(pid.to_owned()))?)
}

/// Mint a stable-ish unique page id from the document, the current time, and the
/// title (no RNG dependency in the workspace), mirroring source-id minting.
fn mint_page_id(state: &AppState, document: &str, detail: &str) -> String {
    let seed = format!("{document}|{}|{detail}", state.store.now());
    let hash = sha256_hex(seed.as_bytes());
    format!("page-{}", &hash[..16])
}
