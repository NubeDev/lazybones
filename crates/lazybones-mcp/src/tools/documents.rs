//! Document tools — documents, pages, references, sources, branding, asset
//! metadata, render, and publish (design §6.2).
//!
//! All mutators check `Capability::Document`, the same guard the `document_*`
//! routes use. Asset *bytes* are not an MCP concern — uploads stay on REST
//! raw-body `POST /assets`; these tools carry only metadata + reference-by-id and
//! return the `/assets/:id` URL for the agent to fetch out of band.
//!
//! Each tool is a thin twin of its REST route: authenticate the bearer token to a
//! session, assert `Capability::Document` via [`McpServer::authorize`], call the
//! existing `StoreHandle` verb (and, for publish, the shared `gh`/`git` helper),
//! serialize the domain type. The render/assembly logic mirrors
//! `routes/document_render.rs` so the HTML an agent previews matches the REST one.

use std::path::Path as FsPath;

use rmcp::handler::server::tool::Extension;
use rmcp::handler::server::wrapper::{Json, Parameters};
use rmcp::{tool, tool_router};
use serde_json::{Value, json};

use lazybones_auth::Capability;
use lazybones_gh::Gh;
use lazybones_render::{Assembled, Brand, Colors, Fonts, ImageAsset, RenderOptions};
use lazybones_store::{
    Branding, DocKind, DocRepo, Document, Page, Source, append_position, sha256_hex,
};

use crate::args::{
    BrandingArgs, DocumentAddPageArgs, DocumentAddSourceArgs, DocumentAttachReferenceArgs,
    DocumentCreateArgs, DocumentPublishArgs, DocumentRefArgs, DocumentRenderArgs,
    DocumentSetRepoArgs, DocumentUpdateArgs, DocumentUpdatePageArgs, IdArgs, ProjectArgs,
};
use crate::auth::authorization_header;
use crate::error::{McpError, McpResult};
use crate::server::McpServer;
use crate::tools::json;

/// The owner kind the document attachment seam keys off (references + sources).
const OWNER_KIND: &str = "document";
/// The attachment thing-kind marking a merged-in reference page.
const REFERENCE_KIND: &str = "reference";
/// The attachment thing-kind marking a source behind a document.
const SOURCE_KIND: &str = "source";

#[tool_router(router = documents_router, vis = "pub(crate)")]
impl McpServer {
    /// `document.create` — author a document (or reusable `reference` page). The twin
    /// of `POST /documents`: requires `Capability::Document`. Conflicts if the id is
    /// taken.
    #[tool(
        name = "document.create",
        description = "Author a branded markdown document (or a reusable reference page). Requires the Document capability (twin of POST /documents)."
    )]
    pub async fn document_create(
        &self,
        Extension(parts): Extension<http::request::Parts>,
        Parameters(args): Parameters<DocumentCreateArgs>,
    ) -> McpResult<Json<Value>> {
        self.authorize(authorization_header(&parts), Capability::Document)?;
        let mut document = Document::new(
            &args.id,
            &args.title,
            DocKind::parse(args.kind.as_deref()),
            self.store().now(),
        );
        document.branding_id = args.branding_id;
        document.project = args.project;
        let created = self
            .store()
            .create_document(&document)
            .await
            .map_err(McpError::from)?;
        json(created)
    }

    /// `document.update` — overwrite a document's authored fields. The twin of
    /// `PUT /documents/:id`: requires `Document`. `404` if unknown; `created_at`, the
    /// project scope, and the GitHub `repo` linkage are preserved.
    #[tool(
        name = "document.update",
        description = "Overwrite a document's authored fields (title, kind, branding). Requires the Document capability (twin of PUT /documents/:id). created_at and the repo linkage are preserved."
    )]
    pub async fn document_update(
        &self,
        Extension(parts): Extension<http::request::Parts>,
        Parameters(args): Parameters<DocumentUpdateArgs>,
    ) -> McpResult<Json<Value>> {
        self.authorize(authorization_header(&parts), Capability::Document)?;
        // Carry forward the non-authored linkage (project + GitHub repo) the routes
        // also preserve, so a content edit never clears it.
        let existing = self.require_document(&args.id).await?;
        let mut document = Document::new(
            &args.id,
            &args.title,
            DocKind::parse(args.kind.as_deref()),
            self.store().now(),
        );
        document.branding_id = args.branding_id;
        document.project = existing.project;
        document.repo = existing.repo;
        let updated = self
            .store()
            .update_document(&document)
            .await
            .map_err(McpError::from)?;
        json(updated)
    }

    /// `document.get` — fetch one document. An open read, twin of `GET /documents/:id`.
    /// `404` if unknown.
    #[tool(
        name = "document.get",
        description = "Fetch one document (title, kind, branding, repo linkage). No capability required (twin of GET /documents/:id)."
    )]
    pub async fn document_get(
        &self,
        Parameters(args): Parameters<IdArgs>,
    ) -> McpResult<Json<Value>> {
        json(self.require_document(&args.id).await?)
    }

    /// `document.list` — list documents, optionally `?project=`. An open read, twin of
    /// `GET /documents`.
    #[tool(
        name = "document.list",
        description = "List documents, optionally narrowed by project scope. No capability required (twin of GET /documents)."
    )]
    pub async fn document_list(
        &self,
        Parameters(args): Parameters<ProjectArgs>,
    ) -> McpResult<Json<Value>> {
        let docs = self
            .store()
            .list_documents(args.project.as_deref())
            .await
            .map_err(McpError::from)?;
        json(docs)
    }

    /// `document.add_page` — append (or insert) a page. The twin of
    /// `POST /documents/:id/pages`: requires `Document`. `404` if the document is
    /// unknown; with no `position` the page is appended after the current last page.
    #[tool(
        name = "document.add_page",
        description = "Append (or insert at a fractional position) a markdown page into a document. Requires the Document capability (twin of POST /documents/:id/pages)."
    )]
    pub async fn document_add_page(
        &self,
        Extension(parts): Extension<http::request::Parts>,
        Parameters(args): Parameters<DocumentAddPageArgs>,
    ) -> McpResult<Json<Value>> {
        self.authorize(authorization_header(&parts), Capability::Document)?;
        self.require_document(&args.document_id).await?;

        // Default to appending after the last page when no explicit position is given.
        let position = match args.position {
            Some(p) => p,
            None => {
                let last = self
                    .store()
                    .list_pages(&args.document_id)
                    .await
                    .map_err(McpError::from)?
                    .last()
                    .map(|p| p.position);
                append_position(last)
            }
        };
        let pid = self.mint_id("page", &args.document_id, &args.title);
        let page = Page::new(
            &pid,
            &args.document_id,
            &args.title,
            &args.body,
            position,
            self.store().now(),
        );
        let created = self.store().create_page(&page).await.map_err(McpError::from)?;
        json(created)
    }

    /// `document.update_page` — overwrite a page's fields and/or move it. The twin of
    /// `PUT /documents/:id/pages/:pid`: requires `Document`. `404` if the page is not
    /// under the document; `created_at` is preserved, position holds unless supplied.
    #[tool(
        name = "document.update_page",
        description = "Overwrite a page's fields and/or move it to a new fractional position. Requires the Document capability (twin of PUT /documents/:id/pages/:pid)."
    )]
    pub async fn document_update_page(
        &self,
        Extension(parts): Extension<http::request::Parts>,
        Parameters(args): Parameters<DocumentUpdatePageArgs>,
    ) -> McpResult<Json<Value>> {
        self.authorize(authorization_header(&parts), Capability::Document)?;
        self.require_document(&args.document_id).await?;
        let existing = self.require_page(&args.document_id, &args.page_id).await?;

        let mut page = Page::new(
            &args.page_id,
            &args.document_id,
            &args.title,
            &args.body,
            existing.position,
            self.store().now(),
        );
        // Move only when a new position is supplied; otherwise hold its place.
        if let Some(position) = args.position {
            page.position = position;
        }
        let updated = self.store().update_page(&page).await.map_err(McpError::from)?;
        json(updated)
    }

    /// `document.list_pages` — a document's pages in render order. An open read, twin
    /// of `GET /documents/:id/pages`. `404` if the document is unknown.
    #[tool(
        name = "document.list_pages",
        description = "List a document's pages in render (position) order. No capability required (twin of GET /documents/:id/pages)."
    )]
    pub async fn document_list_pages(
        &self,
        Parameters(args): Parameters<DocumentRefArgs>,
    ) -> McpResult<Json<Value>> {
        self.require_document(&args.document_id).await?;
        let pages = self
            .store()
            .list_pages(&args.document_id)
            .await
            .map_err(McpError::from)?;
        json(pages)
    }

    /// `document.attach_reference` — merge a reference page into this document. The
    /// twin of `POST /documents/:id/references`: requires `Document`. Idempotent;
    /// `404` if the document is unknown.
    #[tool(
        name = "document.attach_reference",
        description = "Merge a reusable reference page into a document's rendered output (idempotent). Requires the Document capability (twin of POST /documents/:id/references)."
    )]
    pub async fn document_attach_reference(
        &self,
        Extension(parts): Extension<http::request::Parts>,
        Parameters(args): Parameters<DocumentAttachReferenceArgs>,
    ) -> McpResult<Json<Value>> {
        self.authorize(authorization_header(&parts), Capability::Document)?;
        self.require_document(&args.document_id).await?;
        let row = self
            .store()
            .attach(OWNER_KIND, &args.document_id, REFERENCE_KIND, &args.reference_id)
            .await
            .map_err(McpError::from)?;
        json(row)
    }

    /// `document.list_references` — a document's merged-in references, in attach
    /// order. An open read, twin of `GET /documents/:id/references`. `404` if unknown.
    #[tool(
        name = "document.list_references",
        description = "List a document's merged-in reference pages, in attach order. No capability required (twin of GET /documents/:id/references)."
    )]
    pub async fn document_list_references(
        &self,
        Parameters(args): Parameters<DocumentRefArgs>,
    ) -> McpResult<Json<Value>> {
        self.require_document(&args.document_id).await?;
        let rows = self
            .store()
            .list_attachments(OWNER_KIND, &args.document_id, Some(REFERENCE_KIND))
            .await
            .map_err(McpError::from)?;
        json(rows)
    }

    /// `document.add_source` — add a **link** source behind a document. The twin of
    /// `POST /documents/:id/sources` (JSON link path only — file *bytes* stay on the
    /// REST raw-body route, design §6.2): requires `Document`. `404` if unknown.
    #[tool(
        name = "document.add_source",
        description = "Add a link source behind a document (context material that never renders). Requires the Document capability (twin of POST /documents/:id/sources, link path). File-upload sources stay on the REST raw-body route."
    )]
    pub async fn document_add_source(
        &self,
        Extension(parts): Extension<http::request::Parts>,
        Parameters(args): Parameters<DocumentAddSourceArgs>,
    ) -> McpResult<Json<Value>> {
        self.authorize(authorization_header(&parts), Capability::Document)?;
        self.require_document(&args.document_id).await?;

        let sid = self.mint_id("source", &args.document_id, &args.url);
        let title = if args.title.trim().is_empty() {
            args.url.clone()
        } else {
            args.title.clone()
        };
        let source = Source::link(&sid, &args.document_id, &args.url, &title, self.store().now());
        let created = self
            .store()
            .create_source(&source)
            .await
            .map_err(McpError::from)?;
        // Mirror the link onto the generic attachment seam too (thing_kind="source"),
        // so reverse lookups work uniformly across references and sources.
        self.store()
            .attach(OWNER_KIND, &args.document_id, SOURCE_KIND, &created.id)
            .await
            .map_err(McpError::from)?;
        json(created)
    }

    /// `document.list_sources` — the sources behind a document, newest first. An open
    /// read, twin of `GET /documents/:id/sources`. `404` if the document is unknown.
    #[tool(
        name = "document.list_sources",
        description = "List the sources behind a document (links + uploaded files), newest first. No capability required (twin of GET /documents/:id/sources)."
    )]
    pub async fn document_list_sources(
        &self,
        Parameters(args): Parameters<DocumentRefArgs>,
    ) -> McpResult<Json<Value>> {
        self.require_document(&args.document_id).await?;
        let sources = self
            .store()
            .list_sources(&args.document_id)
            .await
            .map_err(McpError::from)?;
        json(sources)
    }

    /// `document.render` — the assembled HTML preview (body + merged references) with
    /// the brand colors/fonts/logo applied, returned as text. An open read, twin of
    /// `GET /documents/:id/render`. An optional `branding_id` previews a brand other
    /// than the saved one (blank ⇒ the default). `404` if the document is unknown.
    #[tool(
        name = "document.render",
        description = "Render a document to assembled HTML (body + merged references, brand colors/fonts/logo applied), returned as text. No capability required (twin of GET /documents/:id/render)."
    )]
    pub async fn document_render(
        &self,
        Parameters(args): Parameters<DocumentRenderArgs>,
    ) -> McpResult<Json<Value>> {
        let doc = self.require_document(&args.id).await?;
        let options = RenderOptions {
            page_numbers: args.page_numbers,
            index: args.index,
        };
        let assembled = self
            .assemble(&doc, args.branding_id.as_deref(), options)
            .await?;
        let html = lazybones_render::render_html(&assembled);
        json(json!({ "html": html }))
    }

    /// `branding.create` — author a reusable brand profile. The twin of
    /// `POST /branding`: requires `Document`. Conflicts if the id is taken.
    #[tool(
        name = "branding.create",
        description = "Author a reusable brand profile (logo + colors + fonts + header/footer). Requires the Document capability (twin of POST /branding)."
    )]
    pub async fn branding_create(
        &self,
        Extension(parts): Extension<http::request::Parts>,
        Parameters(args): Parameters<BrandingArgs>,
    ) -> McpResult<Json<Value>> {
        self.authorize(authorization_header(&parts), Capability::Document)?;
        let mut branding = Branding::new(&args.id, &args.name, self.store().now());
        branding.project = args.project;
        branding.logo_asset_id = args.logo_asset_id;
        branding.colors = args.colors.into();
        branding.fonts = args.fonts.into();
        branding.header_text = args.header_text;
        branding.footer_text = args.footer_text;
        let created = self
            .store()
            .create_branding(&branding)
            .await
            .map_err(McpError::from)?;
        json(created)
    }

    /// `branding.update` — overwrite a brand profile. The twin of `PUT /branding/:id`:
    /// requires `Document`. `404` if unknown; `created_at` is preserved server-side.
    #[tool(
        name = "branding.update",
        description = "Overwrite a reusable brand profile wholesale (created_at preserved). Requires the Document capability (twin of PUT /branding/:id)."
    )]
    pub async fn branding_update(
        &self,
        Extension(parts): Extension<http::request::Parts>,
        Parameters(args): Parameters<BrandingArgs>,
    ) -> McpResult<Json<Value>> {
        self.authorize(authorization_header(&parts), Capability::Document)?;
        // `created_at` here is a placeholder; the store preserves the original.
        let mut branding = Branding::new(&args.id, &args.name, self.store().now());
        branding.logo_asset_id = args.logo_asset_id;
        branding.colors = args.colors.into();
        branding.fonts = args.fonts.into();
        branding.header_text = args.header_text;
        branding.footer_text = args.footer_text;
        let updated = self
            .store()
            .update_branding(&branding)
            .await
            .map_err(McpError::from)?;
        json(updated)
    }

    /// `branding.list` — list brand profiles, optionally `?project=`. An open read,
    /// twin of `GET /branding`.
    #[tool(
        name = "branding.list",
        description = "List reusable brand profiles, optionally narrowed by project scope. No capability required (twin of GET /branding)."
    )]
    pub async fn branding_list(
        &self,
        Parameters(args): Parameters<ProjectArgs>,
    ) -> McpResult<Json<Value>> {
        let rows = self
            .store()
            .list_branding(args.project.as_deref())
            .await
            .map_err(McpError::from)?;
        json(rows)
    }

    /// `asset.list` — list asset metadata, optionally `?project=`. An open read, twin
    /// of `GET /assets`. Bytes are never carried over MCP (design §6.2).
    #[tool(
        name = "asset.list",
        description = "List asset metadata (id, filename, content type, size, sha256), optionally narrowed by project. No capability required (twin of GET /assets). Bytes stay on REST."
    )]
    pub async fn asset_list(
        &self,
        Parameters(args): Parameters<ProjectArgs>,
    ) -> McpResult<Json<Value>> {
        let assets = self
            .store()
            .list_assets(args.project.as_deref())
            .await
            .map_err(McpError::from)?;
        json(assets)
    }

    /// `asset.get_meta` — one asset's metadata plus its `/assets/:id` URL for the
    /// agent to fetch/serve the bytes out of band. The metadata twin of
    /// `GET /assets/:id` (which serves bytes); MCP returns JSON + the URL only
    /// (design §6.2). `404` if the asset is unknown.
    #[tool(
        name = "asset.get_meta",
        description = "Fetch one asset's metadata plus its /assets/:id URL (the agent fetches the bytes over REST out of band). No capability required. MCP never carries asset bytes (design §6.2)."
    )]
    pub async fn asset_get_meta(
        &self,
        Parameters(args): Parameters<IdArgs>,
    ) -> McpResult<Json<Value>> {
        let asset = self
            .store()
            .get_asset(&args.id)
            .await
            .map_err(McpError::from)?
            .ok_or(McpError::NotFound)?;
        let mut value = serde_json::to_value(&asset)
            .map_err(|e| McpError::Internal(format!("serialize: {e}")))?;
        if let Value::Object(map) = &mut value {
            map.insert("url".into(), Value::from(format!("/assets/{}", asset.id)));
        }
        Ok(Json(value))
    }

    /// `document.set_repo` — set the GitHub publishing target. The twin of
    /// `PUT /documents/:id/repo`: requires `Document`. `404` if the document is
    /// unknown; any already-filled `branch`/`*_url` linkage is preserved.
    #[tool(
        name = "document.set_repo",
        description = "Set a document's GitHub publishing target (repo path, base branch, output path). Requires the Document capability (twin of PUT /documents/:id/repo)."
    )]
    pub async fn document_set_repo(
        &self,
        Extension(parts): Extension<http::request::Parts>,
        Parameters(args): Parameters<DocumentSetRepoArgs>,
    ) -> McpResult<Json<Value>> {
        self.authorize(authorization_header(&parts), Capability::Document)?;
        let mut doc = self.require_document(&args.id).await?;
        // Preserve any already-filled linkage (branch/urls) if a repo was set before.
        let prior = doc.repo.clone().unwrap_or_default();
        doc.repo = Some(DocRepo {
            repo: args.repo,
            base_branch: args.base_branch,
            branch_prefix: args.branch_prefix,
            output_path: args.output_path,
            branch: prior.branch,
            issue_url: prior.issue_url,
            pr_url: prior.pr_url,
        });
        let saved = self
            .store()
            .update_document(&doc)
            .await
            .map_err(McpError::from)?;
        json(saved)
    }

    /// `document.publish` — branch → render+commit+push → open a PR, in one call. The
    /// twin of `POST /documents/:id/publish`: requires `Document`. `404` if unknown,
    /// `400` if no repo target is set. Persists `branch` + `pr_url`.
    #[tool(
        name = "document.publish",
        description = "Publish a document: branch off the base, render + commit + push the markdown, then open a PR — in one call. Requires the Document capability (twin of POST /documents/:id/publish)."
    )]
    pub async fn document_publish(
        &self,
        Extension(parts): Extension<http::request::Parts>,
        Parameters(args): Parameters<DocumentPublishArgs>,
    ) -> McpResult<Json<Value>> {
        self.authorize(authorization_header(&parts), Capability::Document)?;
        let doc = self.require_document(&args.id).await?;
        let mut repo = doc.repo.clone().ok_or_else(|| {
            McpError::bad_request(
                "document has no repo target; document.set_repo first",
            )
        })?;
        let gh = Gh::new();

        // 1) Branch off the base.
        let name = branch_name(&repo, &args.id);
        gh.create_branch(&repo.repo, &name, repo.base_branch.as_deref())
            .await
            .map_err(McpError::from)?;
        let branch = gh.current_branch(&repo.repo).await.map_err(McpError::from)?;
        repo.branch = Some(branch.clone());

        // 2) Render + write + commit + push.
        let markdown = self.assemble_markdown(&doc).await?;
        let dest = FsPath::new(&repo.repo).join(&repo.output_path);
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| McpError::Internal(format!("create output dir: {e}")))?;
        }
        std::fs::write(&dest, markdown.as_bytes())
            .map_err(|e| McpError::Internal(format!("write output file: {e}")))?;
        gh.git(&repo.repo, ["add", repo.output_path.as_str()])
            .await
            .map_err(McpError::from)?;
        let message = args
            .message
            .unwrap_or_else(|| format!("docs: publish {}", doc.title));
        gh.git(&repo.repo, ["commit", "-m", message.as_str()])
            .await
            .map_err(McpError::from)?;
        gh.git(&repo.repo, ["push", "-u", "origin", branch.as_str()])
            .await
            .map_err(McpError::from)?;

        // 3) Open the PR.
        let base = repo
            .base_branch
            .clone()
            .unwrap_or_else(|| "main".to_owned());
        let title = args.title.unwrap_or_else(|| doc.title.clone());
        let pr_body = args
            .body
            .unwrap_or_else(|| format!("Rendered document `{}`.", doc.id));
        let url = gh
            .pr_create(&repo.repo, &title, &pr_body, &branch, &base, false)
            .await
            .map_err(McpError::from)?;
        repo.pr_url = Some(url.clone());

        // Persist the new linkage onto the document.
        let mut doc = doc;
        doc.repo = Some(repo);
        self.store()
            .update_document(&doc)
            .await
            .map_err(McpError::from)?;
        json(json!({ "branch": branch, "pr_url": url }))
    }
}

// ---- shared helpers (the in-crate twin of the route module's privates) --------

impl McpServer {
    /// 404 unless the document exists — the twin of the routes' `require_document`.
    async fn require_document(&self, id: &str) -> Result<Document, McpError> {
        self.store()
            .get_document(id)
            .await
            .map_err(McpError::from)?
            .ok_or(McpError::NotFound)
    }

    /// 404 unless the page exists *and* belongs to `document` — the twin of the page
    /// route's `require_page`.
    async fn require_page(&self, document: &str, pid: &str) -> Result<Page, McpError> {
        self.store()
            .get_page(pid)
            .await
            .map_err(McpError::from)?
            .filter(|p| p.document == document)
            .ok_or(McpError::NotFound)
    }

    /// Mint a stable-ish unique id from a kind prefix, the document, the current
    /// time, and a content discriminator (no RNG dependency in the workspace) —
    /// matching the routes' `mint_page_id`/`mint_source_id`.
    fn mint_id(&self, prefix: &str, document: &str, detail: &str) -> String {
        let seed = format!("{document}|{}|{detail}", self.store().now());
        let hash = sha256_hex(seed.as_bytes());
        format!("{prefix}-{}", &hash[..16])
    }

    /// Assemble a document's renderable markdown as one blank-line-joined blob — the
    /// twin of the render route's `assemble_markdown` (own pages, then merged
    /// references in attach order). Used by `document.publish`.
    async fn assemble_markdown(&self, doc: &Document) -> Result<String, McpError> {
        let titled = self.assemble_titled_pages(doc).await?;
        Ok(titled
            .into_iter()
            .map(|(_, body)| body)
            .collect::<Vec<_>>()
            .join("\n\n"))
    }

    /// The non-empty `(title, body)` pages of one document, in render order: the
    /// document's own pages (by position) followed by each attached `reference`
    /// document's pages (attach order — `list_attachments` is newest-first, so we
    /// reverse). The twin of the render route's `assemble_titled_pages`.
    async fn assemble_titled_pages(
        &self,
        doc: &Document,
    ) -> Result<Vec<(String, String)>, McpError> {
        let mut pages = self.titled_page_bodies(&doc.id).await?;

        let mut refs = self
            .store()
            .list_attachments(OWNER_KIND, &doc.id, Some(REFERENCE_KIND))
            .await
            .map_err(McpError::from)?;
        refs.reverse(); // attach order: oldest first
        for att in refs {
            if self
                .store()
                .get_document(&att.thing_id)
                .await
                .map_err(McpError::from)?
                .is_some()
            {
                pages.extend(self.titled_page_bodies(&att.thing_id).await?);
            }
        }
        Ok(pages)
    }

    /// The non-empty `(title, body)` pages of one document, in `position` order.
    async fn titled_page_bodies(
        &self,
        document: &str,
    ) -> Result<Vec<(String, String)>, McpError> {
        Ok(self
            .store()
            .list_pages(document)
            .await
            .map_err(McpError::from)?
            .into_iter()
            .filter(|p| !p.body.trim().is_empty())
            .map(|p| (p.title, p.body))
            .collect())
    }

    /// Resolve a document into the render crate's pure [`Assembled`] input — the twin
    /// of the render route's `assemble`. `brand_override`: `Some(raw)` previews that
    /// brand (blank ⇒ the default/no profile), `None` uses the document's saved
    /// `branding_id`. Logo + inline-image bytes are fetched through the shared blob
    /// store when one is wired in; without it (the unit-test path) images are omitted.
    async fn assemble(
        &self,
        doc: &Document,
        brand_override: Option<&str>,
        options: RenderOptions,
    ) -> Result<Assembled, McpError> {
        let titled = self.assemble_titled_pages(doc).await?;
        let (page_titles, pages): (Vec<String>, Vec<String>) = titled.into_iter().unzip();
        let combined = pages.join("\n\n");

        // A live override (blank ⇒ default) wins over the saved brand.
        let branding_id = match brand_override {
            Some(raw) => Some(raw.trim())
                .filter(|s| !s.is_empty())
                .map(str::to_owned),
            None => doc.branding_id.clone(),
        };
        let branding = match branding_id.as_deref() {
            Some(id) => self.store().get_branding(id).await.map_err(McpError::from)?,
            None => None,
        };
        let brand = branding.as_ref().map(to_brand).unwrap_or_default();

        // Resolve the brand logo bytes, then every inline `![alt](src)` image — only
        // if a blob store is wired in (the mount wires `AppState::assets`).
        let mut logo = None;
        let mut images = Vec::new();
        if self.assets().is_some() {
            if let Some(asset_id) = branding.as_ref().and_then(|b| b.logo_asset_id.as_deref()) {
                logo = self.fetch_image(asset_id, asset_id).await?;
            }
            for src in lazybones_render::image_sources(&combined) {
                if let Some(image) = self.fetch_image(&asset_id_from_src(&src), &src).await? {
                    images.push(image);
                }
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

    /// Fetch an asset's metadata + bytes and package it as the render crate's
    /// [`ImageAsset`] under the markdown `src` it satisfies — the twin of the render
    /// route's `fetch_image`. A missing asset/blob resolves to `None` (the image
    /// simply doesn't render) rather than failing the whole render.
    async fn fetch_image(
        &self,
        asset_id: &str,
        src: &str,
    ) -> Result<Option<ImageAsset>, McpError> {
        let Some(assets) = self.assets() else {
            return Ok(None);
        };
        let Some(asset) = self.store().get_asset(asset_id).await.map_err(McpError::from)? else {
            return Ok(None);
        };
        match assets.get(&asset.sha256, asset.project.as_deref()).await {
            Ok(bytes) => Ok(Some(ImageAsset::new(src, asset.filename, bytes))),
            Err(lazybones_store::AssetError::NotFound(_)) => Ok(None),
            Err(e) => Err(McpError::from(e)),
        }
    }
}

/// The branch name for a document: `{branch_prefix|"doc/"}{id}` — the twin of the
/// publish route's `branch_name`.
fn branch_name(repo: &DocRepo, id: &str) -> String {
    let prefix = repo.branch_prefix.clone().unwrap_or_else(|| "doc/".to_owned());
    format!("{prefix}{id}")
}

/// Map the store's [`Branding`] onto the render crate's pure [`Brand`] — the twin of
/// the render route's `to_brand`.
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

/// Extract an asset id from a markdown image `src`: `/assets/<id>` or `assets/<id>`
/// yield `<id>`; anything else is a bare asset id. The twin of the render route's
/// `asset_id_from_src`.
fn asset_id_from_src(src: &str) -> String {
    let trimmed = src.trim_start_matches('/');
    trimmed
        .strip_prefix("assets/")
        .unwrap_or(src)
        .trim_start_matches('/')
        .to_owned()
}
