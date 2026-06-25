/** A document is a normal authored document, or a reusable `reference` page
 *  (e.g. Terms & Conditions) merged into other documents' rendered output. */
export type DocKind = "document" | "reference";

/** Mirror of `lazybones_store::DocRepo` — the optional GitHub publishing target
 *  + linkage. The `branch`/`*_url` fields are filled in as `gh/*` actions run. */
export interface DocRepo {
  repo: string;
  base_branch?: string | null;
  branch_prefix?: string | null;
  output_path: string;
  branch?: string | null;
  issue_url?: string | null;
  pr_url?: string | null;
}

/** Mirror of `lazybones_store::Document` — a branded *book*: a container whose
 *  content lives in its ordered {@link Page} rows. */
export interface Document {
  id: string;
  title: string;
  project?: string | null;
  kind: DocKind;
  branding_id?: string | null;
  repo?: DocRepo | null;
  /** Persisted layout: print a page number on every page. */
  page_numbers: boolean;
  /** Persisted layout: prepend a table-of-contents index page. */
  index: boolean;
  created_at: string;
  updated_at: string;
}

/** Mirror of `lazybones_store::Page` — one ordered section (page) of a document.
 *  Pages render in ascending `position`; each is a page-break boundary in the
 *  exported PDF. */
export interface Page {
  id: string;
  document: string;
  project?: string | null;
  title: string;
  body: string;
  position: number;
  /** Render this page even when its body is empty (the "page break" toggle): a
   *  deliberate blank spacer page. Defaults to `true`. */
  page_break: boolean;
  created_at: string;
  updated_at: string;
}

/** What a source *is*: an external link, or an uploaded file. */
export type SourceKind = "link" | "file";

/** Mirror of `lazybones_store::Source` — a document's upload / context material
 *  (links, PDFs, images) that sits *behind* the doc and never renders. PDFs get
 *  their text extracted into `extracted_text`. */
export interface Source {
  id: string;
  document: string;
  project?: string | null;
  kind: SourceKind;
  url?: string | null;
  asset_id?: string | null;
  title: string;
  content_type: string;
  extracted_text?: string | null;
  created_at: string;
}
