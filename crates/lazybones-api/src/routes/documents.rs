//! `/documents` — authored, branded markdown documents (and reusable `reference`
//! pages).
//!
//! CRUD plus the **references** seam: a document merges reusable pages (T&C) into
//! its rendered output by attaching them over the generic
//! [`attachment`](lazybones_store::Attachment) seam with `thing_kind="reference"`
//! (distinct from [sources](super::document_sources), which sit *behind* the doc
//! and never render). Mutations require [`Capability::Document`]; reads are open.

use axum::Json;
use axum::extract::{Path, Query, State};
use lazybones_auth::Capability;
use lazybones_store::{Attachment, Document, StoreError};
use serde::Deserialize;

use crate::dto::{CreateDocumentBody, ProjectQuery, UpdateDocumentBody};
use crate::error::ApiResult;
use crate::extract::Session;
use crate::state::AppState;

/// The owner kind the reference handlers operate on.
const OWNER_KIND: &str = "document";
/// The attachment thing-kind that marks a merged-in reference page.
pub const REFERENCE_KIND: &str = "reference";

/// 404 unless the document exists.
pub(crate) async fn require_document(state: &AppState, id: &str) -> ApiResult<Document> {
    Ok(state
        .store
        .get_document(id)
        .await?
        .ok_or_else(|| StoreError::DocumentNotFound(id.to_owned()))?)
}

/// `GET /documents` — list documents (open read), optionally `?project=`.
pub async fn list_documents(
    State(state): State<AppState>,
    Query(query): Query<ProjectQuery>,
) -> ApiResult<Json<Vec<Document>>> {
    Ok(Json(
        state.store.list_documents(query.project.as_deref()).await?,
    ))
}

/// `POST /documents` — author a document. Requires `Document`. `409` on a taken id.
pub async fn create_document(
    State(state): State<AppState>,
    session: Session,
    Json(body): Json<CreateDocumentBody>,
) -> ApiResult<Json<Document>> {
    session.require(Capability::Document, "document", &body.id)?;
    let mut document = Document::new(&body.id, &body.title, body.kind, &body.body, state.store.now());
    document.branding_id = body.branding_id;
    document.project = body.project;
    Ok(Json(state.store.create_document(&document).await?))
}

/// `GET /documents/:id` — fetch one document (open read), or `404`.
pub async fn get_document(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<Json<Document>> {
    Ok(Json(require_document(&state, &id).await?))
}

/// `PUT /documents/:id` — overwrite a document's authored fields. Requires
/// `Document`. `created_at` and the `repo` linkage are preserved.
pub async fn update_document(
    State(state): State<AppState>,
    session: Session,
    Path(id): Path<String>,
    Json(body): Json<UpdateDocumentBody>,
) -> ApiResult<Json<Document>> {
    session.require(Capability::Document, "document", &id)?;
    // Carry forward the GitHub linkage (set via `/repo` + filled by `gh/*`) — it
    // is not an authored field, so a content edit must not clear it.
    let existing = require_document(&state, &id).await?;
    let mut document =
        Document::new(&id, &body.title, body.kind, &body.body, state.store.now());
    document.branding_id = body.branding_id;
    document.project = existing.project;
    document.repo = existing.repo;
    Ok(Json(state.store.update_document(&document).await?))
}

/// `DELETE /documents/:id` — remove a document. Requires `Document`. Returns
/// whether it existed. Does not cascade to attached references/sources (no FK).
pub async fn delete_document(
    State(state): State<AppState>,
    session: Session,
    Path(id): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    session.require(Capability::Document, "document", &id)?;
    let existed = state.store.delete_document(&id).await?;
    Ok(Json(serde_json::json!({ "deleted": existed })))
}

// ---- references (merged-in reusable pages over the attachment seam) ----------

/// `POST /documents/:id/references` body: the reference document to merge in.
#[derive(Debug, Deserialize)]
pub struct AddReferenceBody {
    /// The id of the `reference` document to merge into this one's output.
    pub reference_id: String,
}

/// `GET /documents/:id/references` — list a document's merged-in references
/// (open read), in attach order. `404` if the document does not exist.
pub async fn list_references(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<Json<Vec<Attachment>>> {
    require_document(&state, &id).await?;
    let rows = state
        .store
        .list_attachments(OWNER_KIND, &id, Some(REFERENCE_KIND))
        .await?;
    Ok(Json(rows))
}

/// `POST /documents/:id/references` — merge a reference page into this document.
/// Requires `Document`. Idempotent; `404` if the document does not exist.
pub async fn add_reference(
    State(state): State<AppState>,
    session: Session,
    Path(id): Path<String>,
    Json(body): Json<AddReferenceBody>,
) -> ApiResult<Json<Attachment>> {
    session.require(Capability::Document, "document", &id)?;
    require_document(&state, &id).await?;
    let row = state
        .store
        .attach(OWNER_KIND, &id, REFERENCE_KIND, &body.reference_id)
        .await?;
    Ok(Json(row))
}

/// `DELETE /documents/:id/references/:ref_id` — un-merge a reference. Requires
/// `Document`. `404` if the document does not exist; returns whether the link
/// existed.
pub async fn remove_reference(
    State(state): State<AppState>,
    session: Session,
    Path((id, ref_id)): Path<(String, String)>,
) -> ApiResult<Json<serde_json::Value>> {
    session.require(Capability::Document, "document", &id)?;
    require_document(&state, &id).await?;
    let existed = state
        .store
        .detach(OWNER_KIND, &id, REFERENCE_KIND, &ref_id)
        .await?;
    Ok(Json(serde_json::json!({ "deleted": existed })))
}
