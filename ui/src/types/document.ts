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

/** Mirror of `lazybones_store::Document` — an authored, branded markdown doc. */
export interface Document {
  id: string;
  title: string;
  project?: string | null;
  kind: DocKind;
  branding_id?: string | null;
  body: string;
  repo?: DocRepo | null;
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
