import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  addLinkSource,
  addReference,
  createDocument,
  createPage,
  deleteDocument,
  deletePage,
  getDocument,
  listDocuments,
  listPages,
  listReferences,
  listSources,
  removeReference,
  removeSource,
  updateDocument,
  updatePage,
  uploadFileSource,
  type DocumentDraft,
  type PageDraft,
} from "@/lib/api/documents";

/** Poll the document list. */
export function useDocuments() {
  return useQuery({
    queryKey: ["documents"],
    queryFn: ({ signal }) => listDocuments(undefined, signal),
    refetchInterval: 8000,
  });
}

/** Fetch one document. Disabled when `id` is absent (the new-document page). */
export function useDocument(id?: string) {
  return useQuery({
    queryKey: ["document", id],
    queryFn: ({ signal }) => getDocument(id as string, signal),
    enabled: id != null,
  });
}

/** Author a new document (`POST /documents`). */
export function useCreateDocument() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ id, draft }: { id: string; draft: DocumentDraft }) =>
      createDocument(id, draft),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["documents"] }),
  });
}

/** Edit a document (`PUT /documents/:id`). */
export function useUpdateDocument() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ id, draft }: { id: string; draft: DocumentDraft }) =>
      updateDocument(id, draft),
    onSuccess: (_data, { id }) => {
      qc.invalidateQueries({ queryKey: ["documents"] });
      qc.invalidateQueries({ queryKey: ["document", id] });
      // The preview ("Rendered from the last saved version") is a separate query;
      // refresh it too so saving immediately re-renders the preview/PDF source.
      qc.invalidateQueries({ queryKey: ["doc-render", id] });
    },
  });
}

/** Delete a document (`DELETE /documents/:id`). */
export function useDeleteDocument() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => deleteDocument(id),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["documents"] }),
  });
}

// ---- pages ------------------------------------------------------------------

/** The ordered pages (content) of a document. */
export function usePages(id?: string) {
  return useQuery({
    queryKey: ["doc-pages", id],
    queryFn: ({ signal }) => listPages(id as string, signal),
    enabled: id != null,
  });
}

/** Invalidate a document's page list and its server-rendered preview together. */
function invalidatePages(qc: ReturnType<typeof useQueryClient>, id: string) {
  qc.invalidateQueries({ queryKey: ["doc-pages", id] });
  qc.invalidateQueries({ queryKey: ["doc-render", id] });
}

/** Append (or insert) a page. */
export function useCreatePage() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ id, draft }: { id: string; draft: PageDraft }) => createPage(id, draft),
    onSuccess: (_d, { id }) => invalidatePages(qc, id),
  });
}

/** Edit a page's content and/or move it (new `position`). */
export function useUpdatePage() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ id, pid, draft }: { id: string; pid: string; draft: PageDraft }) =>
      updatePage(id, pid, draft),
    onSuccess: (_d, { id }) => invalidatePages(qc, id),
  });
}

/** Delete a page. */
export function useDeletePage() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ id, pid }: { id: string; pid: string }) => deletePage(id, pid),
    onSuccess: (_d, { id }) => invalidatePages(qc, id),
  });
}

// ---- references -------------------------------------------------------------

/** The reference pages merged into a document. */
export function useReferences(id?: string) {
  return useQuery({
    queryKey: ["doc-references", id],
    queryFn: ({ signal }) => listReferences(id as string, signal),
    enabled: id != null,
  });
}

/** Merge a reference page into a document. */
export function useAddReference() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ id, referenceId }: { id: string; referenceId: string }) =>
      addReference(id, referenceId),
    onSuccess: (_d, { id }) => {
      qc.invalidateQueries({ queryKey: ["doc-references", id] });
      qc.invalidateQueries({ queryKey: ["doc-render", id] });
    },
  });
}

/** Un-merge a reference page. */
export function useRemoveReference() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ id, refId }: { id: string; refId: string }) =>
      removeReference(id, refId),
    onSuccess: (_d, { id }) => {
      qc.invalidateQueries({ queryKey: ["doc-references", id] });
      qc.invalidateQueries({ queryKey: ["doc-render", id] });
    },
  });
}

// ---- sources ----------------------------------------------------------------

/** The sources (uploads / links) behind a document. */
export function useSources(id?: string) {
  return useQuery({
    queryKey: ["doc-sources", id],
    queryFn: ({ signal }) => listSources(id as string, signal),
    enabled: id != null,
  });
}

/** Add a link source. */
export function useAddLinkSource() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ id, url, title }: { id: string; url: string; title?: string }) =>
      addLinkSource(id, url, title),
    onSuccess: (_d, { id }) =>
      qc.invalidateQueries({ queryKey: ["doc-sources", id] }),
  });
}

/** Upload a file source (PDF/image). */
export function useUploadFileSource() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ id, file }: { id: string; file: File }) =>
      uploadFileSource(id, file),
    onSuccess: (_d, { id }) =>
      qc.invalidateQueries({ queryKey: ["doc-sources", id] }),
  });
}

/** Remove a source. */
export function useRemoveSource() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ id, sid }: { id: string; sid: string }) => removeSource(id, sid),
    onSuccess: (_d, { id }) =>
      qc.invalidateQueries({ queryKey: ["doc-sources", id] }),
  });
}
