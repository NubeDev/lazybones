//! `GET /documents/:id/render` (assembled HTML preview) and
//! `GET /documents/:id/export.pdf` (Typst PDF).
//!
//! **Assembly lives here in the API**, keeping `lazybones-render` pure: this
//! module resolves a document into the render crate's [`Assembled`] input —
//! merging attached `reference` pages into one markdown blob (in attach order),
//! resolving the brand profile, and fetching the logo + inline image bytes from
//! the [`BlobStore`](lazybones_store::BlobStore). The render crate then turns
//! that pure value into HTML (preview) or a branded Typst PDF. Both reads are
//! open, like the rest of `/tasks`.

use axum::extract::{Path, State};
use axum::http::header::CONTENT_TYPE;
use axum::response::{Html, IntoResponse, Response};
use lazybones_render::{Assembled, Brand, Colors, Fonts, ImageAsset};
use lazybones_store::{Branding, Document};

use crate::error::ApiResult;
use crate::routes::documents::{REFERENCE_KIND, require_document};
use crate::state::AppState;

/// Assemble a document's renderable markdown: its body followed by each attached
/// `reference` document's body, in attach order (`list_attachments` returns
/// newest-first, so we reverse). Shared with the GitHub commit route.
pub(crate) async fn assemble_markdown(state: &AppState, doc: &Document) -> ApiResult<String> {
    let mut markdown = doc.body.clone();
    let mut refs = state
        .store
        .list_attachments("document", &doc.id, Some(REFERENCE_KIND))
        .await?;
    refs.reverse(); // attach order: oldest first
    for att in refs {
        if let Some(reference) = state.store.get_document(&att.thing_id).await? {
            markdown.push_str("\n\n");
            markdown.push_str(&reference.body);
        }
    }
    Ok(markdown)
}

/// Resolve a document into the render crate's pure [`Assembled`] input: assembled
/// markdown, the resolved brand values, and the logo + inline image bytes pulled
/// from the blob store.
async fn assemble(state: &AppState, doc: &Document) -> ApiResult<Assembled> {
    let markdown = assemble_markdown(state, doc).await?;

    let branding = match doc.branding_id.as_deref() {
        Some(id) => state.store.get_branding(id).await?,
        None => None,
    };
    let brand = branding.as_ref().map(to_brand).unwrap_or_default();

    // Resolve the brand logo bytes, if any.
    let logo = match branding.as_ref().and_then(|b| b.logo_asset_id.as_deref()) {
        Some(asset_id) => fetch_image(state, asset_id, asset_id).await?,
        None => None,
    };

    // Resolve inline image bytes for every `![alt](src)` whose `src` names an
    // asset (`/assets/<id>` or a bare asset id).
    let mut images = Vec::new();
    for src in lazybones_render::image_sources(&markdown) {
        if let Some(image) = fetch_image(state, &asset_id_from_src(&src), &src).await? {
            images.push(image);
        }
    }

    Ok(Assembled {
        title: doc.title.clone(),
        markdown,
        brand,
        logo,
        images,
    })
}

/// Fetch an asset's metadata + bytes from the store and blob store, packaging it
/// as the render crate's [`ImageAsset`] under the markdown `src` it satisfies.
/// A missing asset/blob resolves to `None` (the image simply doesn't render)
/// rather than failing the whole export.
async fn fetch_image(state: &AppState, asset_id: &str, src: &str) -> ApiResult<Option<ImageAsset>> {
    let Some(asset) = state.store.get_asset(asset_id).await? else {
        return Ok(None);
    };
    match state
        .assets
        .get(&asset.sha256, asset.project.as_deref())
        .await
    {
        Ok(bytes) => Ok(Some(ImageAsset::new(src, asset.filename, bytes))),
        Err(lazybones_store::AssetError::NotFound(_)) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// Extract an asset id from a markdown image `src`: `/assets/<id>` or
/// `assets/<id>` yield `<id>`; anything else is treated as a bare asset id.
fn asset_id_from_src(src: &str) -> String {
    let trimmed = src.trim_start_matches('/');
    trimmed
        .strip_prefix("assets/")
        .unwrap_or(src)
        .trim_start_matches('/')
        .to_owned()
}

/// Map the store's [`Branding`] onto the render crate's pure [`Brand`].
fn to_brand(b: &Branding) -> Brand {
    Brand {
        colors: Colors {
            primary: b.colors.primary.clone(),
            secondary: b.colors.secondary.clone(),
            accent: b.colors.accent.clone(),
            text: b.colors.text.clone(),
            background: b.colors.background.clone(),
        },
        fonts: Fonts {
            heading: b.fonts.heading.clone(),
            body: b.fonts.body.clone(),
        },
        header_text: b.header_text.clone(),
        footer_text: b.footer_text.clone(),
    }
}

/// `GET /documents/:id/render` — the assembled HTML preview (body + merged
/// references), with brand colors/fonts applied as CSS. `404` if unknown.
pub async fn render_document(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<Html<String>> {
    let doc = require_document(&state, &id).await?;
    let assembled = assemble(&state, &doc).await?;
    Ok(Html(lazybones_render::render_html(&assembled)))
}

/// `GET /documents/:id/export.pdf` — the document as a branded Typst PDF
/// (`application/pdf`). `404` if unknown, `500` if Typst rendering fails.
pub async fn export_pdf(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<Response> {
    let doc = require_document(&state, &id).await?;
    let assembled = assemble(&state, &doc).await?;
    let pdf = lazybones_render::render_pdf(&assembled)?;
    Ok(([(CONTENT_TYPE, "application/pdf")], pdf).into_response())
}
