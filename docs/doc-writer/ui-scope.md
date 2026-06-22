# Doc-writer UI scope

Companion to [`README.md`](README.md) — that doc owns the store/API/render
backend; **this doc owns Phase 4 (UI)** and supersedes the thin UI sketch in the
README's "UI layer" section. It assumes the backend routes in the README exist
and are stable. Nothing here changes backend shape; where the UI needs something
the backend doesn't expose yet, it is called out explicitly under
[Backend touch-points](#backend-touch-points-small-additive).

## Goal

Ship the operator-facing UI for Documents, Sources, Assets, and the
standalone Branding capability — author a branded document, attach reusable
reference pages, manage research sources, preview, export a PDF, and publish to
GitHub. Match the existing dashboard so this reads as the same app, not a bolt-on.

## Conventions this feature MUST follow

Verified against the current `ui/` tree — do not invent new patterns.

- **Stack:** React 19, Vite, TypeScript strict, `@/*` path alias. State = server
  state only via **`@tanstack/react-query`**; no Redux/Zustand. Styling =
  **Tailwind v4** utilities reading oklch tokens from
  [`styles/globals.css`](../../ui/src/styles/globals.css) (`bg-surface`,
  `text-muted-foreground`, `border-border`, `text-status-*`, `rounded-lg`, …).
  Merge classes with `cn()` ([`lib/utils/cn.ts`](../../ui/src/lib/utils/cn.ts)).
- **Icons:** `lucide-react` only. **No new npm dependencies** unless this doc
  explicitly approves one (see [Dependency decisions](#dependency-decisions)).
- **Navigation:** no URL router. A screen is a `View` string in
  [`app/navigation.ts`](../../ui/src/app/navigation.ts), listed in `NAV_ITEMS`,
  and dispatched by the `switch` in
  [`app/router.tsx`](../../ui/src/app/router.tsx)'s `ViewRenderer`. Pages receive
  an `onNavigate` callback for cross-view jumps.
- **API client:** wrap [`lib/api/client.ts`](../../ui/src/lib/api/client.ts)'s
  `request<T>(path, opts)`. Mutations pass `auth: true` (sends
  `Authorization: Bearer <loopToken()>`). Errors throw `ApiError(status, message)`.
- **Data fetching:** one `use-*.ts` hook per entity under
  [`lib/hooks/`](../../ui/src/lib/hooks/), each a thin `useQuery`/`useMutation`
  wrapper with `queryKey` arrays and `onSuccess` cache invalidation. Mirror
  [`use-workflows.ts`](../../ui/src/lib/hooks/use-workflows.ts). Use
  `refetchInterval` **only** where state changes server-side without user action
  (GitHub publish status); documents/branding/assets are user-edited — no polling.
- **Components:** compose the existing kit in
  [`components/ui/`](../../ui/src/components/ui/) — `Button` (CVA variants),
  `Input`, `Dialog`/`DialogContent`, `Card*`, `Tabs`, `DropdownMenu`, `Badge`,
  `EmptyState`, `Skeleton`, `Tooltip`, and `Markdown`
  ([`components/ui/markdown.tsx`](../../ui/src/components/ui/markdown.tsx)) for
  preview rendering. Layout via [`Topbar`](../../ui/src/components/layout/topbar.tsx).
- **Forms/modals:** dialog-based create/edit (`Dialog` + `DialogContent`), local
  `useState` per field, inline `Field({label,hint,children})` helper, submit via a
  mutation, close+reset on success. **No toast library exists** — surface errors
  inline (`text-status-blocked`) and success by closing the dialog + query
  invalidation refreshing the list. Keep it that way.
- **Types:** one file per entity under [`types/`](../../ui/src/types/), PascalCase
  interfaces mirroring the backend wire format (snake_case fields). Co-locate with
  the api module that returns them.
- **Naming:** kebab-case files, PascalCase component exports, `use*` hooks,
  camelCase client fns (`listDocuments`, `createDocument`).

## Dependency decisions

The doc-writer is editor-heavy, which tempts heavy deps. **Default: add none.**

- **Markdown editing → plain `<textarea>` + live preview, NO editor lib.** The
  repo already renders markdown with `Markdown`; pair a monospace `<textarea>`
  (model the styled textarea in
  [`workflows/mention-textarea.tsx`](../../ui/src/features/workflows/mention-textarea.tsx))
  with a side-by-side `Markdown` preview pane. This keeps bundle + risk low and
  matches the "dense operator dashboard" tone. A rich editor (CodeMirror 6,
  TipTap) is a deliberate **phase-2 upgrade** behind the same component boundary —
  not in this cut. (Mirror the README's "ship the cheap path, gate the fancy
  path" stance on Typst.)
- **Color picking → native `<input type="color">` + a hex `Input`.** No
  `react-colorful`/picker dep. Brand colors are a small fixed set
  (primary/secondary/accent/text/background).
- **File upload → native `<input type="file">` + drag-drop via standard DnD
  events.** No dropzone dep.
- **PDF preview → no in-app PDF.js viewer.** "Export PDF" downloads the file
  (and/or opens it in a new tab); the in-app live preview is the HTML render.

If a build agent believes a dep is warranted, stop and raise it — do not add one
silently.

## Backend touch-points (small, additive)

The UI needs two things the README's API section implies but the current `ui/`
client can't yet do. Both are additive and belong in Phase 2/4 wiring, flagged
here so they aren't discovered mid-build:

1. **Binary upload helper.** `request<T>` only serializes JSON bodies. Asset and
   file-source uploads use **raw body + `Content-Type` + `X-Filename` headers**
   (README, "Upload transport"). Add a sibling `upload<T>(path, file, opts)` in
   [`lib/api/client.ts`](../../ui/src/lib/api/client.ts) that sends the raw
   `File`/`Blob` with those headers and the `auth: true` bearer token. Do **not**
   switch the backend to multipart (README explicitly keeps multipart out).
2. **Authed binary GET for export (only if needed).** PDF export
   (`GET /documents/:id/export.pdf`) and `GET /assets/:id` are **reads → unguarded**
   per the README, so a plain `<a href>` / `window.open` against `apiBase()+path`
   works with no token. Use that. Only if a later decision guards these reads does
   the UI need a blob-fetch-with-bearer download helper — note it, don't build it.

## File inventory (new)

```
ui/src/
  app/
    navigation.ts          (edit) + "documents", "branding" Views & NAV_ITEMS
    router.tsx             (edit) + cases → DocumentsPage, BrandingPage
  lib/
    api/
      client.ts           (edit) + upload<T>() raw-body helper
      documents.ts        list/get/create/update/delete + references + render + repo/gh/publish
      assets.ts           list/get/upload/delete  (uses upload<T>)
      branding.ts         list/get/create/update/delete
      sources.ts          list/add-link/upload-file/delete (per document)
    hooks/
      use-documents.ts    queries + mutations (incl. references, gh actions)
      use-assets.ts
      use-branding.ts
      use-sources.ts
  types/
    document.ts           Document, DocKind, DocRepo, DocumentSummary
    asset.ts              Asset
    branding.ts           Branding, BrandColors, BrandFonts
    source.ts             Source, SourceKind
  components/
    brand-picker.tsx      reusable "pick a brand" combobox (app-wide, not doc-only)
    asset-input.tsx       reusable upload-or-pick-asset control (logo/image fields)
  features/
    documents/
      documents-page.tsx    list + create; toggle Documents vs Reference pages
      document-card.tsx     summary card (title, kind badge, brand swatch, repo state)
      document-dialog.tsx   create dialog (title, kind, branding)
      document-editor.tsx   detail: tabbed editor shell
      document-body.tsx     split textarea editor + live Markdown preview
      document-references.tsx  attach/detach/reorder reference pages
      document-sources.tsx     uploads/links panel + extracted-text preview
      document-repo.tsx        repository + publish panel (reuses workflow pieces)
      assets-library.tsx       upload/list/delete assets (logos, images)
    branding/
      branding-page.tsx     list/create many brand profiles
      branding-editor.tsx   edit one: name, logo, colors, fonts, header/footer + live preview
      brand-swatch.tsx      small color-row preview used in cards/pickers
```

## Screens

### 1. Documents page — `features/documents/documents-page.tsx`

List/detail toggle exactly like
[`workflows-page.tsx`](../../ui/src/features/workflows/workflows-page.tsx):
`selected: string|null` → render `DocumentEditor` else the list.

- **Topbar:** title "Documents", subtitle count, action = `DocumentDialog`.
- **Sub-filter:** a `Tabs` or segmented control switching the grid between
  **Documents** (`kind=Document`) and **Reference pages** (`kind=Reference`) —
  both come from `GET /documents`, filter client-side on `kind`. Reference pages
  are authored with the same editor; the only difference is the kind badge and
  that they show up in the attach-reference picker.
- **Grid:** responsive cards (`grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3
  gap-4`). `DocumentCard` shows title, `Badge` for kind, a small brand swatch
  (resolve `branding_id`), and a repo/publish indicator (none / branch / PR
  open). `EmptyState` (lucide `BookOpen`/`Files`) when empty; `Skeleton` grid
  while loading; `ApiError` message on failure.
- **Create dialog** (`document-dialog.tsx`): `title` Input, `kind` select
  (Document | Reference), optional `BrandPicker`. On success → `onCreated(id)`
  opens the editor.

### 2. Document editor — `document-editor.tsx`

Detail shell with a back button (`onBack`) and a `Tabs` strip:

- **Edit** → `document-body.tsx`
- **References** → `document-references.tsx`
- **Sources** → `document-sources.tsx`
- **Repository / Publish** → `document-repo.tsx`

Header row: editable title (`Input`, debounced `PUT`), `BrandPicker`, and the
two export actions:
- **Preview** — already shown live in the Edit tab.
- **Export PDF** — `<a href={apiBase()+"/documents/"+id+"/export.pdf"}>` (download
  attr); unguarded GET, no token needed.

Saving model: autosave on blur/debounce via `useUpdateDocument` (PUT), with a
small "saved/saving" indicator — no explicit Save button (matches the app's
low-chrome feel). Keep dirty state local; invalidate `["document", id]` on success.

#### 2a. Body editor — `document-body.tsx`

Two-pane split: left = monospace `<textarea>` bound to `body` markdown; right =
live preview. Live preview has two modes via a toggle:
- **Local** (default, instant): render the textarea content with the existing
  `Markdown` component — fast feedback, no network.
- **Assembled** (accurate): `GET /documents/:id/render` returns the server HTML
  with **merged reference pages** + brand styling. Fetch on demand (button /
  tab focus), not on every keystroke. This is the only faithful preview of what
  the PDF contains, so make it obvious.

"Insert asset" action: opens the asset picker (`AssetInput`/library), inserts a
markdown image referencing `GET /assets/:id` at the cursor.

#### 2b. References — `document-references.tsx`

Manage merged reusable pages over the attachment seam. Model
[`template_attachments`](../../ui/src/features/workflows/) usage but for docs:
- List attached references (in attach order — this is render order) with
  detach buttons; reorder is phase-2 (note it, don't block).
- "Attach reference" picker: lists `kind=Reference` documents not already
  attached → `POST /documents/:id/references`.
- Make the **references vs sources** distinction explicit in copy here: a short
  hint "References are merged into the exported PDF" — because the adjacent
  Sources tab is the opposite (never rendered).

#### 2c. Sources — `document-sources.tsx`

The uploads/research panel (context behind the doc, never rendered):
- **Add link:** url + title Inputs → `POST /documents/:id/sources` (kind Link).
- **Upload file:** `<input type="file">` / drag-drop → `upload()` to
  `POST /documents/:id/sources` (kind File). PDFs/images accepted.
- **List:** each source row shows kind icon, title, content-type. For PDF
  sources, an expandable **extracted-text preview** (the backend's
  `extracted_text`) — collapsed by default, this is the substrate the future RAG
  phase uses, so surface it but keep it unobtrusive.
- File sources render/download via `GET /assets/:id` (the blob is an asset).
- Delete per source. Copy hint: "Sources are context for authoring — they are
  not included in the exported document."

#### 2d. Repository / Publish — `document-repo.tsx`

Reuse the workflow GitHub building blocks directly:
- **Repo target** (`PUT /documents/:id/repo`): reuse
  [`repo-picker.tsx`](../../ui/src/features/workflows/repo-picker.tsx) for the
  local checkout path, [`branch-field.tsx`](../../ui/src/features/workflows/branch-field.tsx)
  for base branch, plus Inputs for `branch_prefix` and `output_path`
  (default e.g. `docs/<id>.md`).
- **Actions** (each a `Capability::Document`-guarded POST, `auth: true`):
  - Create branch (`/gh/branch`), Create issue (`/gh/issue`),
    Create commit (`/gh/commit`), Open PR (`/gh/pr`).
  - **Publish** (`/publish`): one-click branch → commit → PR. Primary button.
- **Result links:** show persisted `branch`, `issue_url`, `pr_url` as external
  links once set (model
  [`workflow-issues.tsx`](../../ui/src/features/workflows/workflow-issues.tsx) /
  [`workflow-prs.tsx`](../../ui/src/features/workflows/workflow-prs.tsx)).
- This is the **one place** that benefits from light polling: after a publish
  action, `refetchInterval` the document until `pr_url` lands, then stop.

### 3. Assets library — `assets-library.tsx`

Reachable from the document editor's insert-asset flow and as a standalone panel
(a tab on the Documents page or its own minor nav entry — prefer a tab to avoid
nav clutter). Upload (drag-drop / file input → `upload()` to `POST /assets`),
grid of thumbnails (`<img src={apiBase()+"/assets/"+id}>`), filename +
content-type + size, delete. Content-addressed dedup is server-side — on
duplicate upload the existing asset comes back; reflect that gracefully (no error).

### 4. Branding — `features/branding/branding-page.tsx`

**Standalone, app-wide** (its own `View` + `NAV_ITEMS` entry, lucide `Palette`),
not nested under Documents. Many brand profiles.

- **List/create:** cards showing name + `BrandSwatch` (color row) + logo
  thumbnail; create dialog with just a name (then edit).
- **Editor** (`branding-editor.tsx`): name; **logo** via the reusable
  `AssetInput` (upload or pick an existing asset → stores `logo_asset_id`);
  **colors** (primary/secondary/accent/text/background) via native color input +
  hex `Input`; **fonts** (heading/body) as text Inputs; **header_text** /
  **footer_text** Inputs. A **live brand preview** card renders a mini
  letterhead (logo + header + a sample heading/paragraph in the chosen colors) so
  the user sees the brand without exporting a PDF.
- `seed.rs` guarantees one default brand exists, so pickers are never empty.

### 5. Reusable `BrandPicker` — `components/brand-picker.tsx`

App-wide component (lives in `components/`, not `features/`, because the README
mandates branding be reusable by any future surface): a combobox/select over
`GET /branding` showing name + swatch, value = `branding_id`, with an inline
"Manage branding →" `onNavigate("branding")` shortcut. First consumer is the
document editor's brand field; built so theming/other surfaces drop it in later.

## Auth / capability in the UI

- **Reads unguarded:** all `GET`s (documents, assets bytes, render, export.pdf,
  branding, sources) need no token — plain `request`/`<a href>`.
- **Mutations guarded by `Capability::Document`:** every create/update/delete and
  all `gh/*`/`publish` calls pass `auth: true` so the bearer `loopToken()` is
  attached, exactly like workflow mutations today. The token already carries the
  capability after the README's `loop_grant()` change — the UI does no
  capability logic itself, it just sends the token. A `403` surfaces as the
  standard inline `ApiError` message.

## Build sub-phases (UI = README Phase 4)

Land in this order on the shared feature branch; each step compiles + the app
runs before the next.

1. **Plumbing:** `client.ts` `upload()` helper; types
   (`document/asset/branding/source.ts`); api clients; hooks. No screens yet —
   verify with a throwaway call.
2. **Branding first** (it's the dependency of the document editor's brand field):
   `branding` View + nav + router, branding page/editor, `BrandSwatch`,
   `BrandPicker`, `AssetInput`, assets library. Verifiable on its own.
3. **Documents core:** `documents` View + nav + router, documents page + card +
   create dialog, editor shell, body editor with local + assembled preview,
   export download.
4. **Documents references + sources:** the two tabs over the attachment seam +
   the upload/link/extracted-text flows.
5. **Publish:** repo/publish tab reusing repo-picker/branch-field + gh actions +
   result links + post-publish polling.

## Verification

Builds on the README's curl E2E; this is the browser pass (`npm run dev` in
`ui/`, against a local `lazybonesd`):

1. **Branding:** create a brand, upload a logo via `AssetInput`, set colors/fonts
   + header/footer; confirm the live brand preview updates; confirm it appears in
   `BrandPicker`.
2. **Assets:** upload an image in the library; re-upload the same file → confirm
   it dedups (same asset, no error); confirm the thumbnail loads from
   `GET /assets/:id`.
3. **Reference page:** author a `kind=Reference` doc (e.g. T&C).
4. **Document:** create a Document, pick the brand, write markdown; confirm the
   **Local** preview is instant and the **Assembled** preview shows the merged
   reference; attach the reference in the References tab.
5. **Sources:** add a link and upload a PDF in the Sources tab; confirm the PDF's
   extracted-text preview renders and that sources do **not** appear in the
   Assembled preview (context, not output).
6. **Export:** click Export PDF → file downloads; open it → logo, brand colors,
   merged T&C present.
7. **Publish:** set a repo (repo-picker + base branch + output path), click
   Publish → branch/commit/PR happen; the PR link appears on the panel and the
   post-publish poll stops once `pr_url` lands. Exercise Create issue too.
8. **Build gate:** `npm run build` (tsc `--noEmit` + vite) is clean; no new npm
   dependency was added beyond those approved here (none).

## Phase-2 (designed-in, not built)

Noted so reviewers know the seams are intentional: rich markdown editor
(CodeMirror/TipTap) behind `document-body.tsx`; reference **reordering** + inline
`{{include:id}}` markers; in-app PDF.js viewer; project scoping (`?project=`
filter UI) once [`projects.md`](../lazybones-server/projects.md) lands;
RAG/source-search UI over the pre-declared `source_chunk` vector table.
