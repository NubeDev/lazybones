import { apiBase, loopToken } from "./config";
import { ApiError, request } from "./client";
import type { DocKind, DocRepo, Document, Source } from "@/types/document";
import type { Attachment } from "@/types/skill";

/** The authored fields of a document (everything the editor controls). */
export interface DocumentDraft {
  title: string;
  kind: DocKind;
  branding_id?: string | null;
  body: string;
}

/** `GET /documents` — list documents (open read), optionally `?project=`. */
export function listDocuments(project?: string, signal?: AbortSignal): Promise<Document[]> {
  const q = project ? `?project=${encodeURIComponent(project)}` : "";
  return request<Document[]>(`/documents${q}`, { signal });
}

/** `GET /documents/:id` — fetch one document; `404` if absent. */
export function getDocument(id: string, signal?: AbortSignal): Promise<Document> {
  return request<Document>(`/documents/${encodeURIComponent(id)}`, { signal });
}

/** `POST /documents` — author a document. `409` if the id is taken. */
export function createDocument(id: string, draft: DocumentDraft): Promise<Document> {
  return request<Document>("/documents", {
    method: "POST",
    auth: true,
    body: { id, ...draft },
  });
}

/** `PUT /documents/:id` — overwrite a document's authored fields. The `repo`
 *  linkage and `created_at` are preserved server-side. */
export function updateDocument(id: string, draft: DocumentDraft): Promise<Document> {
  return request<Document>(`/documents/${encodeURIComponent(id)}`, {
    method: "PUT",
    auth: true,
    body: draft,
  });
}

/** `DELETE /documents/:id` — remove a document. */
export function deleteDocument(id: string): Promise<{ deleted: boolean }> {
  return request<{ deleted: boolean }>(`/documents/${encodeURIComponent(id)}`, {
    method: "DELETE",
    auth: true,
  });
}

// ---- references (reusable pages merged into the rendered output) -------------

/** `GET /documents/:id/references` — the document's merged-in reference pages. */
export function listReferences(id: string, signal?: AbortSignal): Promise<Attachment[]> {
  return request<Attachment[]>(`/documents/${encodeURIComponent(id)}/references`, { signal });
}

/** `POST /documents/:id/references` — merge a reference page in (idempotent). */
export function addReference(id: string, referenceId: string): Promise<Attachment> {
  return request<Attachment>(`/documents/${encodeURIComponent(id)}/references`, {
    method: "POST",
    auth: true,
    body: { reference_id: referenceId },
  });
}

/** `DELETE /documents/:id/references/:refId` — un-merge a reference. */
export function removeReference(id: string, refId: string): Promise<{ deleted: boolean }> {
  return request<{ deleted: boolean }>(
    `/documents/${encodeURIComponent(id)}/references/${encodeURIComponent(refId)}`,
    { method: "DELETE", auth: true },
  );
}

// ---- sources (uploads / context material behind the doc) --------------------

/** `GET /documents/:id/sources` — list the sources behind a document. */
export function listSources(id: string, signal?: AbortSignal): Promise<Source[]> {
  return request<Source[]>(`/documents/${encodeURIComponent(id)}/sources`, { signal });
}

/** `POST /documents/:id/sources` (JSON) — add a link source. */
export function addLinkSource(id: string, url: string, title = ""): Promise<Source> {
  return request<Source>(`/documents/${encodeURIComponent(id)}/sources`, {
    method: "POST",
    auth: true,
    body: { url, title },
  });
}

/** `POST /documents/:id/sources` (raw body) — upload a file source (PDF/image).
 *  PDFs have their text extracted into `extracted_text` server-side. */
export async function uploadFileSource(id: string, file: File): Promise<Source> {
  const res = await fetch(`${apiBase()}/documents/${encodeURIComponent(id)}/sources`, {
    method: "POST",
    headers: {
      "content-type": file.type || "application/octet-stream",
      "x-filename": file.name,
      authorization: `Bearer ${loopToken()}`,
    },
    body: file,
  });
  if (!res.ok) {
    const text = await res.text().catch(() => "");
    throw new ApiError(res.status, text || `${res.status} ${res.statusText}`);
  }
  return (await res.json()) as Source;
}

/** `DELETE /documents/:id/sources/:sid` — remove a source. */
export function removeSource(id: string, sid: string): Promise<{ deleted: boolean }> {
  return request<{ deleted: boolean }>(
    `/documents/${encodeURIComponent(id)}/sources/${encodeURIComponent(sid)}`,
    { method: "DELETE", auth: true },
  );
}

// ---- render / export --------------------------------------------------------

/** `GET /documents/:id/render` — the assembled HTML preview (body + merged
 *  references, brand CSS applied). Returns the raw HTML string. */
export async function renderDocumentHtml(id: string, signal?: AbortSignal): Promise<string> {
  const res = await fetch(`${apiBase()}/documents/${encodeURIComponent(id)}/render`, { signal });
  if (!res.ok) {
    const text = await res.text().catch(() => "");
    throw new ApiError(res.status, text || `${res.status} ${res.statusText}`);
  }
  return res.text();
}

/** The absolute URL of a document's PDF export (`GET /documents/:id/export.pdf`,
 *  open read) — point a download link or new tab at it. */
export function exportPdfUrl(id: string): string {
  return `${apiBase()}/documents/${encodeURIComponent(id)}/export.pdf`;
}

// ---- GitHub publishing ------------------------------------------------------

/** `PUT /documents/:id/repo` — set the GitHub publishing target. */
export function setDocRepo(
  id: string,
  body: { repo: string; base_branch?: string | null; branch_prefix?: string | null; output_path: string },
): Promise<Document> {
  return request<Document>(`/documents/${encodeURIComponent(id)}/repo`, {
    method: "PUT",
    auth: true,
    body,
  });
}

/** `POST /documents/:id/gh/branch` — create the document's branch off its base. */
export function createDocBranch(id: string): Promise<{ branch: string }> {
  return request<{ branch: string }>(`/documents/${encodeURIComponent(id)}/gh/branch`, {
    method: "POST",
    auth: true,
  });
}

/** `POST /documents/:id/gh/commit` — render + write + commit (optional push). */
export function commitDoc(
  id: string,
  body: { message?: string; push?: boolean } = {},
): Promise<{ committed: boolean; pushed: boolean; output_path: string }> {
  return request(`/documents/${encodeURIComponent(id)}/gh/commit`, {
    method: "POST",
    auth: true,
    body,
  });
}

/** `POST /documents/:id/gh/pr` — open a PR for the document's branch. */
export function createDocPr(
  id: string,
  body: { title?: string; body?: string; draft?: boolean } = {},
): Promise<{ url: string }> {
  return request<{ url: string }>(`/documents/${encodeURIComponent(id)}/gh/pr`, {
    method: "POST",
    auth: true,
    body,
  });
}

/** `POST /documents/:id/gh/issue` — open an issue from the document. */
export function createDocIssue(
  id: string,
  body: { title?: string; body?: string } = {},
): Promise<{ url: string }> {
  return request<{ url: string }>(`/documents/${encodeURIComponent(id)}/gh/issue`, {
    method: "POST",
    auth: true,
    body,
  });
}

/** `POST /documents/:id/publish` — branch → commit → PR, in one call. */
export function publishDoc(
  id: string,
  body: { message?: string; title?: string; body?: string } = {},
): Promise<{ branch: string; pr_url: string }> {
  return request(`/documents/${encodeURIComponent(id)}/publish`, {
    method: "POST",
    auth: true,
    body,
  });
}

export type { DocRepo };
