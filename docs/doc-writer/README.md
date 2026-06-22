# Document writer + file/asset server for lazybones

## Context

lazybones today orchestrates multi-agent build work (tasks/workflows over an
embedded SurrealDB store, an axum REST surface, a React UI). There is no way to
**author and produce documents** — branded PDFs assembled from reusable content
(e.g. a quote with standard Terms & Conditions merged in), with a place to store
**images/assets** (logos, diagrams) once and reuse them.

This adds first-class **Documents**, **Assets** (a file server), and per-document
**Sources** (uploads), plus PDF export and optional **GitHub publishing** (set a
repo on a document, then branch → commit the rendered doc → open a PR/issue via
the existing `lazybones-gh` wrapper) — and a **global, standalone Branding**
capability that is *not* part of the document writer. It deliberately follows the
existing "one module per entity" store pattern and the "verb-per-file" REST
pattern so it slots in without inventing new architecture.

**Branding is cross-cutting, not a doc-writer subfeature.** The user maintains
**many** brand profiles (logo + colors + fonts + header/footer) as a general,
app-wide resource. Any feature — the PDF exporter today, app/UI theming and other
surfaces later — references a brand by id; the user just **picks which branding to
use** wherever a brand is relevant. The document writer is merely the first
consumer. Branding therefore gets its own store entity, its own top-level REST
surface (`/branding`), its own UI home, and a reusable **BrandPicker** component.

**Decisions locked with the user:**
- **PDF via Typst** (pure Rust, no external binary).
- **AI authoring is out of scope for now** — backend + UI only. (The doc/asset
  model is built so an AI authoring turn can be added later via the existing
  management-agent/skill plumbing.)
- **Backend + UI** both in this cut.
- **Asset bytes:** content-addressed blob store behind a small `BlobStore` trait,
  default backend = files under the data dir; metadata row in SurrealDB. Rationale
  below.
- **Projects-readiness:** every new entity carries an optional `project` scope
  from day one (today always `None`), so the future `projects.md` work slots in
  with no migration.

## Resolved open questions (no remaining ambiguity)

These were verified against the code and decided with the user so a build agent
never has to guess. (This doc previously had no OQ list; the rest of the repo
tracks open questions explicitly — these are now closed.)

- **Auth/capability:** there is **no "GitHub-mutating" capability** today — the
  existing `/gh/*` routes use `Capability::Author`. **Decision: add one new
  `Capability::Document` variant** (in
  [`lazybones-auth/src/capability.rs`](crates/lazybones-auth/src/capability.rs))
  and grant it in `loop_grant()` and the `Author`/`AuthorAndManage`
  `ManagementProfile`s. **All** document/asset/branding mutations *and* the
  `gh/*` document-publish routes are guarded by `Capability::Document`
  (publishing a document is a document action). Reads are unguarded, like
  `/tasks`. Do **not** reuse `Author` (it means "edit task records") and do
  **not** split a separate `Publish` capability (premature).
- **Typst is the main risk and a clean slate** (nothing in the repo uses
  `typst`/`pulldown-cmark`/`pdf-extract`). **Decision: a de-risk spike (Phase
  3a) lands before the full render layer** — pin exact crate versions and prove
  `typst::compile` → PDF bytes with embedded fonts in a throwaway test. Only
  after it compiles do we build the markdown→Typst converter. `render_html`
  (markdown→HTML+CSS) is the cheap fallback the export path can ship on if Typst
  iteration runs long.
- **`BlobStore` trait** is `async` and returns `Result<_, AssetError>` (a new
  error type), mirroring how store verbs return `Result<_, StoreError>`. A new
  `AssetError` enum gets a `From<AssetError> for ApiError` impl in
  [`lazybones-api/src/error.rs`](crates/lazybones-api/src/error.rs) (→ 404 for
  not-found, 500 for IO), matching the existing `StoreError` mapping.
- **Enum serialization:** `DocKind`, `SourceKind`, and `BrandColors`/`BrandFonts`
  follow the existing convention — JSON-struct columns serialized as a `String`
  like [`SkillRow.action`](crates/lazybones-store/src/skill/row.rs); kind enums
  serialize lowercase like [`MergeMode`](crates/lazybones-store/src/run/model.rs)
  (`#[serde(rename_all = "lowercase")]`).
- **Asset serving** (`GET /assets/:id`): buffer the blob (`BlobStore::get`) and
  return it with the stored `content_type` — no streaming in this cut (assets are
  logos/images, not large files). Streaming is a transparent later swap behind
  the trait.
- **Upload transport:** raw body + `Content-Type` / `X-Filename` headers,
  confirmed — `multipart` is **not** a workspace dependency and this keeps it
  that way.
- **New crate wiring:** `crates/lazybones-render` is added to the root
  `Cargo.toml` `members` array (edition 2024, Rust 1.93) and inherits workspace
  deps via `{ workspace = true }`, exactly like
  [`lazybones-engine`](crates/lazybones-engine/Cargo.toml).

## Asset storage — why a blob store, not bytes-in-DB

The user deferred to "whatever is better long term." Storing large binary blobs
inline in the embedded SurrealKV store bloats it and routes every byte through the
KV value layer. Better long term: keep **bytes outside the relational rows**,
content-addressed by sha256 (this is what makes "reusable images" dedup for free),
behind a trait so the backend can change without touching callers:

- `asset` table (SurrealDB): metadata only — `id, project, filename, content_type,
  size, sha256, created_at`.
- `BlobStore` trait: `put(sha256, bytes)`, `get(sha256) -> bytes`, `delete`.
  Default impl writes under `{data_dir}/assets/{project-prefix}/{sha256}`.
- Swappable later to S3 or a SurrealDB `DEFINE BUCKET` backend without changing
  the asset metadata, routes, or UI. Project becomes a key prefix.

## Store layer — `crates/lazybones-store`

Mirror the [`skill`](crates/lazybones-store/src/skill/) module shape exactly
(`model.rs` domain type, `row.rs` SurrealDB row with `RecordId` + `Option`
columns + `from_*`/`into_*`, verb-per-file `create/get/list/update/delete.rs`,
`mod.rs` re-exports + tests, optional `seed.rs`).

New modules:

1. **`branding/`** (standalone, global, many profiles) — `Branding { id,
   project: Option<String>, name, logo_asset_id: Option<String>, colors (struct:
   primary/secondary/accent/text/background…), fonts (heading/body), header_text,
   footer_text, created_at, updated_at }`. Colors/fonts serialized as a JSON
   column like [`SkillRow.action`](crates/lazybones-store/src/skill/row.rs).
   Install-wide and reusable by **any** feature (PDF export now, UI theming and
   other surfaces later) — a consumer stores a `branding_id` and resolves it.
   `seed.rs` seeds one neutral default brand so there is always one to pick.

2. **`asset/`** — metadata CRUD only (bytes go through `BlobStore`).
   `Asset { id, project, filename, content_type, size, sha256, created_at }`.
   `create_asset` is content-addressed: if a row with the same `sha256` (+project)
   exists, return it (dedup = reusable images). Plus a `BlobStore` trait +
   default file-backed impl (new `asset/blob.rs`).

3. **`document/`** — `Document { id, title, project, kind: DocKind
   (Document | Reference), branding_id: Option<String>, body: String (markdown),
   repo: Option<DocRepo>, created_at, updated_at }`. "Reusable pages" (T&C) are
   documents with `kind = Reference`. **`DocRepo`** is the optional GitHub target
   + linkage (mirrors the workflow [`Workspace`](crates/lazybones-store/src/run/model.rs)
   + task issue-linkage shape): `{ repo: String (local checkout path),
   base_branch: Option<String>, branch_prefix: Option<String>, output_path:
   String (where the rendered doc is committed, e.g. `docs/<id>.md`), branch:
   Option<String>, issue_url: Option<String>, pr_url: Option<String> }`. The
   `*_url`/`branch` fields are filled in as GitHub actions run, like the task's
   `set_issue_link`.

4. **`source/`** — a document's **uploads / context material** (links, PDFs,
   images the author adds *behind* the doc; not rendered into the output).
   `Source { id, document, project, kind: SourceKind (Link | File), url:
   Option<String> (links), asset_id: Option<String> (uploaded files via the asset
   server), title, content_type, extracted_text: Option<String>, created_at }`.
   Files reuse the same blob store + sha256 dedup. On **PDF** upload, extract plain
   text (`pdf-extract` crate) into `extracted_text` now — powers preview/keyword
   search and is the exact substrate the later RAG phase chunks + embeds. The
   source↔document link rides the same `attachment` seam (`thing_kind="source"`).

**Two distinct concepts (don't conflate):** *references* are reusable pages
**merged into the rendered PDF** (T&C); *sources* are research material **behind**
the doc that never renders. Both ride the generic attachment seam with different
`thing_kind`s (`reference` vs `source`).

**Vectors / RAG — deferred (designed-in, not built):** embedding PDFs into a
SurrealDB vector index only pays off once AI/RAG consumes them, and that needs the
embedding-provider decision the store already flags (the `memory` vector table is
pre-declared for this). So now: store `extracted_text` only. Pre-declare a
`source_chunk` SCHEMALESS table with a `vector` (`array<float>`) field + HNSW
index in [`init_schema.rs`](crates/lazybones-store/src/init_schema.rs) — a no-op
today (like `memory`), so the RAG layer lands later with **no migration**.

**Merging reusable pages:** reuse the existing generic
[`attachment`](crates/lazybones-store/src/attachment/) seam — no new join table.
A document includes a reference via `attach(owner_kind="document", owner_id=doc,
thing_kind="reference", thing_id=ref)`; the renderer appends referenced pages (in
attach order) after the body. `list_attachments` already powers the "references on
this doc" picker and reverse lookup. (Inline positional `{{include:id}}` markers
are a clean phase-2 enhancement; appending covers the T&C case.)

Supporting edits:
- [`init_schema.rs`](crates/lazybones-store/src/init_schema.rs): add `document`,
  `asset`, `branding`, `source` SCHEMALESS tables + a `project` field/index on
  each, an `asset` index on `sha256`, and the deferred `source_chunk` vector table
  (+ HNSW index) as a no-op seam. `attachment` is already defined.
- [`error.rs`](crates/lazybones-store/src/error.rs): add
  `DocumentNotFound/Exists`, `AssetNotFound`, `BrandingNotFound/Exists`,
  `SourceNotFound`.
- [`handle.rs`](crates/lazybones-store/src/handle.rs) + [`lib.rs`](crates/lazybones-store/src/lib.rs):
  wire the new verbs/types as `StoreHandle` methods and public exports, exactly
  like skills are wired today.
- Per-module tests mirroring [`skill/mod.rs`](crates/lazybones-store/src/skill/mod.rs)
  tests (roundtrip, dup-is-error, update-preserves-created_at, seed idempotent).

## Render layer — new crate `crates/lazybones-render`

Isolated crate so Typst's heavy deps don't slow the api/store build, and so
rendering is pure + unit-testable. It takes an **already-assembled** document
(title, branding values, resolved markdown, resolved logo/image bytes) — no store
dependency.

- Deps: `typst` + `typst-pdf` + `typst-assets` (embedded fonts) + `comemo`;
  `pulldown-cmark` to turn markdown into Typst markup. A `typst-as-lib`-style
  `World` impl (or the `typst-as-lib` crate) supplies fonts + the template.
- `render_pdf(assembled) -> Vec<u8>`: build a branded `.typ` (logo image, brand
  colors, header/footer) and `typst::compile` → PDF bytes.
- `render_html(assembled) -> String`: markdown→HTML + brand CSS, for in-UI preview.
- **Main implementation risk:** markdown→Typst markup conversion (pulldown-cmark
  AST → Typst). Budget the bulk of render effort here; HTML preview is the cheap
  fallback path if Typst integration needs iteration.

Assembly (resolve attached references into one markdown blob, fetch the logo +
inline image bytes from `BlobStore`) lives in the API alongside the export route,
keeping `lazybones-render` pure.

## API layer — `crates/lazybones-api`

Verb-per-file routes under [`routes/`](crates/lazybones-api/src/routes/), wired in
[`routes/mod.rs`](crates/lazybones-api/src/routes/mod.rs); DTOs in `src/dto`;
new `StoreError` arms mapped in [`src/error.rs`](crates/lazybones-api/src/error.rs).
Add an `assets: AssetStore` (the `BlobStore`) field to
[`AppState`](crates/lazybones-api/src/state.rs), constructed in
[`serve.rs`](crates/lazybones-cli/src/serve.rs) from `config.data_dir`.

- **Documents:** `GET/POST /documents`, `GET/PUT/DELETE /documents/:id`,
  `GET/POST/DELETE /documents/:id/references…` (over the attachment seam, like
  [`template_attachments`](crates/lazybones-api/src/routes/template_attachments.rs)),
  `GET /documents/:id/render` (assembled HTML preview),
  `GET /documents/:id/export.pdf` (Typst PDF, `application/pdf`).
- **Assets (file server):** `POST /assets` (raw body upload + `Content-Type` /
  `X-Filename` headers — avoids pulling axum's `multipart` feature into the lean
  workspace config; sha256 + dedup server-side), `GET /assets`,
  `GET /assets/:id` (serve bytes with stored content-type — this is the asset
  server endpoint, also used as the logo/image source), `DELETE /assets/:id`.
- **Sources (uploads):** `GET/POST /documents/:id/sources` (add a link, or upload
  a file → blob store + sha256 + PDF text extraction), `DELETE
  /documents/:id/sources/:sid`. File bytes served via the existing `/assets/:id`.
- **GitHub publishing (reuses [`lazybones-gh::Gh`](crates/lazybones-gh/src/lib.rs),
  already an api dep via [`routes/gh.rs`](crates/lazybones-api/src/routes/gh.rs)):**
  - `PUT /documents/:id/repo` — set the document's repo target (local path, base
    branch, branch prefix, output path).
  - `POST /documents/:id/gh/branch` — `Gh::create_branch` off the base branch
    (`branch_prefix` + doc id); persist `branch`.
  - `POST /documents/:id/gh/commit` — render the document and write it to
    `output_path` in the repo, then `git add`/`commit` (via `Gh::git`); optional
    push. (Markdown by default; PDF optional.)
  - `POST /documents/:id/gh/pr` — `Gh::pr_create` (title from doc title, body from
    a summary); persist `pr_url`.
  - `POST /documents/:id/gh/issue` — `Gh::issue_create` from the document; persist
    `issue_url`.
  - `POST /documents/:id/publish` — convenience: branch → commit → PR in one call.
  - Guarded by `Capability::Document` (see Resolved open questions) — the same new
    capability that guards every document/asset/branding mutation.
- **Branding:** `GET/POST /branding`, `GET/PUT/DELETE /branding/:id`.
- All list endpoints accept an optional `?project=` filter (no-op until projects
  land, but the seam is there).

## UI layer — `ui/`

> **Detailed UI scope lives in [`ui-scope.md`](ui-scope.md)** — that doc owns
> Phase 4 (screens, components, dependency decisions, build sub-phases, browser
> verification) and supersedes the sketch below. The summary here is kept for
> orientation.

Follow the existing feature-folder + `lib/api/*.ts` + `types/*` conventions
([workflows](ui/src/features/workflows/), [client.ts](ui/src/lib/api/client.ts)).

- Add `"documents"` to `View` in [navigation.ts](ui/src/app/navigation.ts) (a nav
  item, e.g. lucide `BookOpen`/`Files`) and a case in
  [router.tsx](ui/src/app/router.tsx).
- `ui/src/lib/api/{documents,assets,branding}.ts` — typed clients via `request`.
- `ui/src/features/documents/`:
  - `documents-page.tsx` — list/create documents (and a Reference-pages view).
  - `document-editor.tsx` — title, markdown editor, branding picker, attach-
    reference picker, insert-asset, "Export PDF" + live HTML preview.
  - `document-sources.tsx` — the **uploads/sources panel**: add links, upload
    PDFs/images, list them, show extracted-text preview for PDFs, delete.
  - `document-repo.tsx` — the **Repository / Publish panel**: pick the repo
    (reuse [`repo-picker.tsx`](ui/src/features/workflows/repo-picker.tsx)) + base
    branch (reuse [`branch-field.tsx`](ui/src/features/workflows/branch-field.tsx))
    + output path; buttons for Create branch / Create issue / Open PR (and a
    one-click **Publish**); show the resulting branch/issue/PR links (model the
    [`workflow-issues`](ui/src/features/workflows/workflow-issues.tsx)/
    [`workflow-prs`](ui/src/features/workflows/workflow-prs.tsx) views).
  - `assets-library.tsx` — upload/list/delete assets (logos, images).
- **Branding gets its own home (not inside Documents)** — `ui/src/features/
  branding/`: `branding-page.tsx` (list/create/edit many brand profiles, logo
  upload, color pickers, header/footer) reachable from its own nav entry (or under
  Settings). Plus a **reusable `BrandPicker` component** (`ui/src/components/`)
  that any feature can drop in to "select which branding to use" — the document
  editor's brand picker is its first use.

## Projects-readiness (do now, costs little)

- Optional `project` field + index on `document`/`asset`/`branding`; `?project=`
  filter on lists; blob keys prefixed by project. Today everything is `None`.
- When [projects.md](docs/lazybones-server/projects.md) lands, documents/assets/
  branding attach under a project via the same `under`/scope model with no schema
  migration (SCHEMALESS) — set `project` on create and filter by it.
- A document's `DocRepo` falls back to the **project's repo** config once projects
  exist (projects.md notes a project carries its repo target), so a doc in a
  project need not re-specify the repo.

## Build phases

**Execution model (lazybones workflow):** these phases run as a **sequential
chain of tasks sharing ONE worktree and ONE branch** — not independent per-task
PRs. Each task is `depends_on` the previous, claims the *same* `Workspace.repo`
worktree + `branch`, and builds on the prior task's work in place. Do **not** set
`MergeMode::Pr` per task; the branch is opened once and the whole feature lands as
one reviewable branch at the end.

0. **Scope doc**: this document, committed at `docs/doc-writer/README.md`
   (mirroring `docs/lazybones-server/`) — already in the repo, version-controlled
   and reviewable alongside the code. (Phase 0 is done.)
1. **Store**: branding (standalone) → asset (+ `BlobStore`) → document → source
   modules; schema (incl. deferred `source_chunk` seam), errors, handle/lib
   wiring, seeds, per-module tests.
2. **API**: routes, DTOs, error mapping, new `Capability::Document` (+ grants),
   `AppState.assets`, `serve.rs` wiring.
3a. **Render spike (de-risk, gate the rest of render on it):** add
   `crates/lazybones-render` with pinned `typst`/`typst-pdf`/`typst-assets`/
   `comemo` versions and **one** test that compiles a trivial `.typ` → PDF bytes
   with an embedded font. If this fails to compile, stop and resolve the Typst
   API/version before proceeding — this is the only phase that can hard-block.
3b. **Render**: markdown→Typst converter (`pulldown-cmark` AST → Typst markup),
   `render_pdf` + `render_html`, assembly in the API + `export.pdf` route.
4. **UI**: api clients, types, documents feature, **standalone branding section +
   reusable BrandPicker**, asset library, nav + router.

## Verification

- `cargo test -p lazybones-store` (new module roundtrip/dedup/seed tests) and
  `cargo test -p lazybones-api`; `cargo build` (incl. `lazybones-render`).
- End-to-end via curl against a local `lazybonesd`:
  1. `POST /branding` (name + colors), `POST /assets` a logo PNG → note id,
     `PUT /branding/:id` with `logo_asset_id`.
  2. `POST /documents` a Reference page (T&C), then a main Document; attach the
     reference; set `branding_id`.
  3. Add sources: `POST /documents/:id/sources` a link and upload a PDF →
     confirm the PDF's `extracted_text` is populated; `GET .../sources` lists them.
  4. `GET /documents/:id/render` → HTML shows body + merged T&C (sources absent —
     they are context, not rendered output).
  5. `GET /documents/:id/export.pdf -o out.pdf` → open; confirm logo, brand
     colors, and merged T&C render.
  6. `GET /assets/:id` returns the logo bytes with the right content-type.
  7. GitHub flow against a throwaway local repo with a GitHub remote:
     `PUT /documents/:id/repo`, then `POST /documents/:id/publish` → confirm a
     branch is created, the rendered doc is committed at `output_path`, and a PR
     opens; `pr_url` is persisted on the document. Also exercise `gh/issue`.
- Run the UI (`npm run dev` in `ui/`): create branding + upload logo, author a
  document, attach a reference, preview, download the PDF, set a repo and Publish
  to a PR.
