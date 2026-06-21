//! Attachments on a template — the first consumer of the generic
//! [`attachment`](lazybones_store::Attachment) seam.
//!
//! - `POST   /templates/:id/attachments` — attach a thing (body `{thing_kind, thing_id}`).
//! - `DELETE /templates/:id/attachments/:thing_kind/:thing_id` — detach one.
//! - `GET    /templates/:id/attachments` — list (optional `?thing_kind=`).
//!
//! The handlers fix `owner_kind = "template"` and call the generic store verbs, so
//! any other owner can reuse them unchanged. They **do** 404 a missing template
//! (the owner), but deliberately do **not** validate that the attached thing
//! exists — the thing is polymorphic and carries no hard FK (see the
//! [`attachment`](lazybones_store::Attachment) module doc).

use axum::Json;
use axum::extract::{Path, Query, State};
use lazybones_auth::Capability;
use lazybones_store::{Attachment, StoreError};
use serde::Deserialize;

use crate::dto::AttachBody;
use crate::error::ApiResult;
use crate::extract::Session;
use crate::state::AppState;

/// The owner kind these handlers operate on.
const OWNER_KIND: &str = "template";

/// `?thing_kind=skill` filter for the list route.
#[derive(Debug, Default, Deserialize)]
pub struct AttachmentQuery {
    /// Restrict to one thing-kind (e.g. `skill`); all kinds when omitted.
    pub thing_kind: Option<String>,
}

/// 404 unless the template (owner) exists.
async fn require_template(state: &AppState, id: &str) -> ApiResult<()> {
    state
        .store
        .get_template(id)
        .await?
        .ok_or_else(|| StoreError::TemplateNotFound(id.to_owned()))?;
    Ok(())
}

/// `POST /templates/:id/attachments` — attach a thing. Requires `Author`.
/// Idempotent; `404` if the template does not exist.
pub async fn attach_to_template(
    State(state): State<AppState>,
    session: Session,
    Path(id): Path<String>,
    Json(body): Json<AttachBody>,
) -> ApiResult<Json<Attachment>> {
    session.require(Capability::Author, "author", &id)?;
    require_template(&state, &id).await?;
    let row = state
        .store
        .attach(OWNER_KIND, &id, &body.thing_kind, &body.thing_id)
        .await?;
    Ok(Json(row))
}

/// `DELETE /templates/:id/attachments/:thing_kind/:thing_id` — detach a thing.
/// Requires `Author`. `404` if the template does not exist; returns whether the
/// link existed.
pub async fn detach_from_template(
    State(state): State<AppState>,
    session: Session,
    Path((id, thing_kind, thing_id)): Path<(String, String, String)>,
) -> ApiResult<Json<serde_json::Value>> {
    session.require(Capability::Author, "author", &id)?;
    require_template(&state, &id).await?;
    let existed = state
        .store
        .detach(OWNER_KIND, &id, &thing_kind, &thing_id)
        .await?;
    Ok(Json(serde_json::json!({ "deleted": existed })))
}

/// `GET /templates/:id/attachments` — list a template's attachments (open read),
/// optionally narrowed by `?thing_kind=`. `404` if the template does not exist.
pub async fn list_template_attachments(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<AttachmentQuery>,
) -> ApiResult<Json<Vec<Attachment>>> {
    require_template(&state, &id).await?;
    let rows = state
        .store
        .list_attachments(OWNER_KIND, &id, query.thing_kind.as_deref())
        .await?;
    Ok(Json(rows))
}
