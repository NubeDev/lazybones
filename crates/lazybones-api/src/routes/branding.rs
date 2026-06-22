//! `/branding` — the standalone, app-wide brand-profile catalogue.
//!
//! Branding is **not** a document-writer subfeature: the user maintains many
//! reusable brand profiles (logo + colors + fonts + header/footer) that any
//! feature references by id (the PDF exporter today, UI theming later). Mutations
//! require [`Capability::Document`]; reads are open.

use axum::Json;
use axum::extract::{Path, Query, State};
use lazybones_auth::Capability;
use lazybones_store::{Branding, StoreError};

use crate::dto::{CreateBrandingBody, ProjectQuery, UpdateBrandingBody};
use crate::error::ApiResult;
use crate::extract::Session;
use crate::state::AppState;

/// `GET /branding` — list brand profiles (open read), optionally `?project=`.
pub async fn list_branding(
    State(state): State<AppState>,
    Query(query): Query<ProjectQuery>,
) -> ApiResult<Json<Vec<Branding>>> {
    Ok(Json(
        state.store.list_branding(query.project.as_deref()).await?,
    ))
}

/// `POST /branding` — author a brand profile. Requires `Document`. `409` if the
/// id already exists.
pub async fn create_branding(
    State(state): State<AppState>,
    session: Session,
    Json(body): Json<CreateBrandingBody>,
) -> ApiResult<Json<Branding>> {
    session.require(Capability::Document, "document", &body.id)?;
    let mut branding = Branding::new(&body.id, &body.name, state.store.now());
    branding.project = body.project;
    branding.logo_asset_id = body.logo_asset_id;
    branding.colors = body.colors;
    branding.fonts = body.fonts;
    branding.header_text = body.header_text;
    branding.footer_text = body.footer_text;
    Ok(Json(state.store.create_branding(&branding).await?))
}

/// `GET /branding/:id` — fetch one brand profile (open read), or `404`.
pub async fn get_branding(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<Json<Branding>> {
    let branding = state
        .store
        .get_branding(&id)
        .await?
        .ok_or(StoreError::BrandingNotFound(id))?;
    Ok(Json(branding))
}

/// `PUT /branding/:id` — overwrite a brand profile. Requires `Document`. `404` if
/// it does not exist; `created_at` is preserved by the store.
pub async fn update_branding(
    State(state): State<AppState>,
    session: Session,
    Path(id): Path<String>,
    Json(body): Json<UpdateBrandingBody>,
) -> ApiResult<Json<Branding>> {
    session.require(Capability::Document, "document", &id)?;
    // `created_at` here is a placeholder; the store preserves the original.
    let mut branding = Branding::new(&id, &body.name, state.store.now());
    branding.logo_asset_id = body.logo_asset_id;
    branding.colors = body.colors;
    branding.fonts = body.fonts;
    branding.header_text = body.header_text;
    branding.footer_text = body.footer_text;
    Ok(Json(state.store.update_branding(&branding).await?))
}

/// `DELETE /branding/:id` — remove a brand profile. Requires `Document`. Returns
/// whether it existed.
pub async fn delete_branding(
    State(state): State<AppState>,
    session: Session,
    Path(id): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    session.require(Capability::Document, "document", &id)?;
    let existed = state.store.delete_branding(&id).await?;
    Ok(Json(serde_json::json!({ "deleted": existed })))
}
