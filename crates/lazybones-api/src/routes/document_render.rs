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

use axum::extract::{Path, Query, State};
use axum::http::header::CONTENT_TYPE;
use axum::response::{Html, IntoResponse, Response};
use lazybones_render::{Assembled, Brand, Colors, Fonts, ImageAsset, RenderOptions};
use lazybones_store::{Branding, Document};
use serde::Deserialize;

use crate::error::ApiResult;
use crate::routes::documents::{REFERENCE_KIND, require_document};
use crate::state::AppState;

/// Assemble a document's renderable **pages**, in render order: the document's
/// own pages (by `position`) followed by each attached `reference` document's
/// pages, in attach order (`list_attachments` returns newest-first, so we
/// reverse). Each non-empty page is one entry — the render layer puts a page
/// break between them, so each becomes its own PDF page.
pub(crate) async fn assemble_pages(state: &AppState, doc: &Document) -> ApiResult<Vec<String>> {
    Ok(assemble_titled_pages(state, doc)
        .await?
        .into_iter()
        .map(|(_, body)| body)
        .collect())
}

/// Like [`assemble_pages`] but keeps each non-empty page's `(title, body)` so the
/// index can list page titles. Same render order (own pages, then merged
/// references in attach order).
async fn assemble_titled_pages(
    state: &AppState,
    doc: &Document,
) -> ApiResult<Vec<(String, String)>> {
    let mut pages = titled_page_bodies(state, &doc.id).await?;

    let mut refs = state
        .store
        .list_attachments("document", &doc.id, Some(REFERENCE_KIND))
        .await?;
    refs.reverse(); // attach order: oldest first
    for att in refs {
        if state.store.get_document(&att.thing_id).await?.is_some() {
            pages.extend(titled_page_bodies(state, &att.thing_id).await?);
        }
    }
    Ok(pages)
}

/// Assemble a document's renderable markdown as one blank-line-joined blob — for
/// consumers that don't paginate (the committed `.md` file, the issue body).
/// Shared with the GitHub commit/issue routes.
pub(crate) async fn assemble_markdown(state: &AppState, doc: &Document) -> ApiResult<String> {
    Ok(assemble_pages(state, doc).await?.join("\n\n"))
}

/// The non-empty `(title, body)` pages of one document, in `position` (render)
/// order.
async fn titled_page_bodies(state: &AppState, document: &str) -> ApiResult<Vec<(String, String)>> {
    Ok(state
        .store
        .list_pages(document)
        .await?
        .into_iter()
        .filter(|p| !p.body.trim().is_empty())
        .map(|p| (p.title, p.body))
        .collect())
}

/// Which brand to render with. `None` ⇒ the document's saved `branding_id`;
/// `Some` ⇒ a live override (the editor's current, possibly-unsaved selection),
/// where an empty/blank value means "the default brand" (no profile).
type BrandOverride<'a> = Option<&'a str>;

/// Resolve a document into the render crate's pure [`Assembled`] input: assembled
/// markdown, the resolved brand values, and the logo + inline image bytes pulled
/// from the blob store.
async fn assemble(
    state: &AppState,
    doc: &Document,
    brand_override: BrandOverride<'_>,
    options: RenderOptions,
) -> ApiResult<Assembled> {
    let titled = assemble_titled_pages(state, doc).await?;
    let (page_titles, pages): (Vec<String>, Vec<String>) = titled.into_iter().unzip();
    let combined = pages.join("\n\n");

    // The effective brand id: a live override (blank ⇒ default) wins over the
    // saved one, so the preview tracks the picker before the document is saved.
    let branding_id = match brand_override {
        Some(raw) => Some(raw.trim()).filter(|s| !s.is_empty()).map(str::to_owned),
        None => doc.branding_id.clone(),
    };
    let branding = match branding_id.as_deref() {
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
    // asset (`/assets/<id>` or a bare asset id), across all pages.
    let mut images = Vec::new();
    for src in lazybones_render::image_sources(&combined) {
        if let Some(image) = fetch_image(state, &asset_id_from_src(&src), &src).await? {
            images.push(image);
        }
    }

    Ok(Assembled {
        title: doc.title.clone(),
        pages,
        brand,
        logo,
        images,
        page_titles,
        options,
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

/// Query for the render preview: an optional live `branding_id` override so the
/// editor can preview the currently-picked brand before the document is saved,
/// plus the live layout toggles (page numbers / index) so the preview tracks the
/// checkboxes before anything is saved.
#[derive(Debug, Deserialize)]
pub struct RenderQuery {
    /// The brand to preview with (blank ⇒ default brand). Absent ⇒ the saved one.
    #[serde(default)]
    branding_id: Option<String>,
    /// Print a page number on every page.
    #[serde(default)]
    page_numbers: bool,
    /// Prepend a table-of-contents index page.
    #[serde(default)]
    index: bool,
}

impl RenderQuery {
    /// The layout options carried by this query.
    fn options(&self) -> RenderOptions {
        RenderOptions {
            page_numbers: self.page_numbers,
            index: self.index,
        }
    }
}

/// `GET /documents/:id/render` — the assembled HTML preview (body + merged
/// references), with brand colors/fonts/logo applied. `404` if unknown.
///
/// An optional `?branding_id=` query previews a brand other than the saved one
/// (the editor passes its current picker value so the preview is live).
pub async fn render_document(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(q): Query<RenderQuery>,
) -> ApiResult<Html<String>> {
    let doc = require_document(&state, &id).await?;
    let assembled = assemble(&state, &doc, q.branding_id.as_deref(), q.options()).await?;
    Ok(Html(lazybones_render::render_html(&assembled)))
}

/// `GET /documents/:id/export.pdf` — the document as a branded Typst PDF
/// (`application/pdf`). `404` if unknown, `500` if Typst rendering fails.
pub async fn export_pdf(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(q): Query<ExportQuery>,
) -> ApiResult<Response> {
    let doc = require_document(&state, &id).await?;
    // Export always uses the document's saved brand (no live override), but honors
    // the layout toggles passed on the export link so the PDF matches the preview.
    let assembled = assemble(&state, &doc, None, q.options()).await?;
    let pdf = lazybones_render::render_pdf(&assembled)?;
    Ok(([(CONTENT_TYPE, "application/pdf")], pdf).into_response())
}

/// Query for the PDF export: the layout toggles (page numbers / index), so the
/// exported document matches what the editor previewed.
#[derive(Debug, Deserialize)]
pub struct ExportQuery {
    /// Print a page number on every page.
    #[serde(default)]
    page_numbers: bool,
    /// Prepend a table-of-contents index page.
    #[serde(default)]
    index: bool,
}

impl ExportQuery {
    /// The layout options carried by this query.
    fn options(&self) -> RenderOptions {
        RenderOptions {
            page_numbers: self.page_numbers,
            index: self.index,
        }
    }
}
