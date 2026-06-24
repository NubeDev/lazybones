import { useEffect, useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import {
  ArrowLeft,
  BookmarkPlus,
  Eye,
  FileDown,
  FileText,
  FolderGit2,
  ImagePlus,
  Paperclip,
  Plus,
  RefreshCw,
  X,
} from "lucide-react";
import { Topbar } from "@/components/layout/topbar";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Skeleton } from "@/components/ui/skeleton";
import { Tabs, TabsList, TabsTrigger, TabsContent } from "@/components/ui/tabs";
import { BrandPicker } from "@/components/brand-picker";
import { ApiError } from "@/lib/api/client";
import { exportPdfUrl, renderDocumentHtml } from "@/lib/api/documents";
import {
  useAddReference,
  useCreateDocument,
  useDocument,
  useDocuments,
  useReferences,
  useRemoveReference,
  useUpdateDocument,
} from "@/lib/hooks/use-documents";
import type { DocumentDraft } from "@/lib/api/documents";
import type { DocKind, Document } from "@/types/document";
import { DocumentSources } from "./document-sources";
import { DocumentPages } from "./document-pages";
import { DocumentRepo } from "./document-repo";
import { AssetsLibrary } from "./assets-library";

const EMPTY: DocumentDraft = { title: "", kind: "document", branding_id: null };

function draftFrom(d: Document): DocumentDraft {
  return {
    title: d.title,
    kind: d.kind,
    branding_id: d.branding_id ?? null,
  };
}

/** The document editor: title, kind, branding picker, markdown body, attach-
 *  reference picker, insert-asset, plus a live HTML preview and Export PDF. The
 *  sources and repository/publish panels live in side tabs. Pass `documentId`
 *  to edit; pass nothing with `initialKind` to author a new document. */
export function DocumentEditor({
  documentId,
  initialKind = "document",
  onBack,
}: {
  documentId?: string;
  initialKind?: DocKind;
  onBack: () => void;
}) {
  // The id becomes fixed once the document is created; before that we are in
  // "create" mode and persistence-backed tabs are disabled.
  const [currentId, setCurrentId] = useState<string | undefined>(documentId);
  const creating = currentId == null;
  const { data: doc, isLoading } = useDocument(currentId);

  const [id, setId] = useState(documentId ?? "");
  const [draft, setDraft] = useState<DocumentDraft>({ ...EMPTY, kind: initialKind });

  useEffect(() => {
    if (doc) {
      setId(doc.id);
      setDraft(draftFrom(doc));
    }
  }, [doc]);

  const create = useCreateDocument();
  const update = useUpdateDocument();
  const mut = creating ? create : update;

  function save() {
    const trimmed = id.trim();
    if (!trimmed || !draft.title.trim()) return;
    if (creating) {
      create.mutate({ id: trimmed, draft }, { onSuccess: (d) => setCurrentId(d.id) });
    } else {
      update.mutate({ id: trimmed, draft });
    }
  }

  const err = mut.error;
  const message =
    err instanceof ApiError
      ? err.status === 409
        ? `A document "${id.trim()}" already exists.`
        : err.message
      : err
        ? "Something went wrong."
        : null;

  if (!creating && !doc && isLoading) {
    return (
      <div className="flex h-full flex-col">
        <Topbar title="Document" subtitle="Loading…" />
        <div className="space-y-3 p-6">
          <Skeleton className="h-6 w-64" />
          <Skeleton className="h-72 w-full" />
        </div>
      </div>
    );
  }

  const canSave = !!id.trim() && !!draft.title.trim() && !mut.isPending;
  const kindLabel = draft.kind === "reference" ? "Reference page" : "Document";

  return (
    <div className="flex h-full min-w-0 flex-col">
      <Topbar
        title={creating ? `New ${kindLabel.toLowerCase()}` : (doc?.title ?? id)}
        subtitle={creating ? "Author a branded markdown document" : `${kindLabel} · ${currentId}`}
        actions={
          <div className="flex items-center gap-2">
            <Button variant="ghost" size="sm" onClick={onBack}>
              <ArrowLeft /> Back
            </Button>
            {!creating && (
              <Button asChild variant="secondary" size="sm">
                <a href={exportPdfUrl(currentId!)} target="_blank" rel="noreferrer">
                  <FileDown /> Export PDF
                </a>
              </Button>
            )}
            <Button size="sm" onClick={save} disabled={!canSave}>
              {creating ? "Create" : "Save changes"}
            </Button>
          </div>
        }
      />

      <div className="grid min-h-0 flex-1 grid-cols-1 gap-0 lg:grid-cols-2">
        {/* Left: the editor. */}
        <div className="min-h-0 overflow-y-auto border-r border-border">
          <div className="space-y-4 p-5">
            <Field
              label="Document id"
              hint={creating ? "lowercase, unique, e.g. q3-quote-acme" : "fixed once created"}
            >
              <Input
                value={id}
                autoFocus={creating}
                disabled={!creating}
                onChange={(e) => setId(e.target.value)}
                placeholder="q3-quote-acme"
                className="font-mono"
              />
            </Field>

            <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
              <Field label="Title">
                <Input
                  value={draft.title}
                  onChange={(e) => setDraft({ ...draft, title: e.target.value })}
                  placeholder="Q3 Quote — Acme Corp"
                />
              </Field>
              <Field label="Kind">
                <select
                  value={draft.kind}
                  onChange={(e) => setDraft({ ...draft, kind: e.target.value as DocKind })}
                  className="h-9 w-full rounded-md border border-border bg-surface-2 px-3 text-sm outline-none focus-visible:border-accent/50 focus-visible:ring-2 focus-visible:ring-ring/40"
                >
                  <option value="document">Document</option>
                  <option value="reference">Reference page (reusable)</option>
                </select>
              </Field>
            </div>

            <Field label="Branding" hint="which brand profile to render with">
              <BrandPicker
                value={draft.branding_id ?? null}
                onChange={(b) => setDraft({ ...draft, branding_id: b })}
              />
            </Field>

            <div>
              <div className="mb-1 flex items-center gap-1.5">
                <FileText className="size-3.5 text-accent" />
                <span className="text-xs font-medium">Pages</span>
              </div>
              {creating ? (
                <p className="rounded-md border border-dashed border-border px-3 py-6 text-center text-xs text-muted-foreground">
                  Create the {kindLabel.toLowerCase()} first to add and edit pages.
                </p>
              ) : (
                <DocumentPages documentId={currentId!} />
              )}
            </div>

            {message && (
              <p className="rounded-md border border-status-blocked/30 bg-status-blocked/10 px-3 py-2 text-xs text-status-blocked">
                {message}
              </p>
            )}
          </div>
        </div>

        {/* Right: preview + side panels. */}
        <div className="min-h-0 overflow-y-auto">
          {creating ? (
            <div className="flex h-full items-center justify-center p-8 text-center">
              <p className="max-w-xs text-xs text-muted-foreground">
                Create the document to enable live preview, references, sources, and publishing.
              </p>
            </div>
          ) : (
            <SidePanels
              documentId={currentId!}
              document={doc}
              brandingId={draft.branding_id ?? null}
            />
          )}
        </div>
      </div>
    </div>
  );
}

function SidePanels({
  documentId,
  document,
  brandingId,
}: {
  documentId: string;
  document?: Document;
  brandingId: string | null;
}) {
  return (
    <Tabs defaultValue="preview" className="flex h-full flex-col">
      <div className="border-b border-border p-2">
        <TabsList>
          <TabsTrigger value="preview">
            <Eye className="size-3.5" /> Preview
          </TabsTrigger>
          <TabsTrigger value="references">
            <BookmarkPlus className="size-3.5" /> References
          </TabsTrigger>
          <TabsTrigger value="sources">
            <Paperclip className="size-3.5" /> Sources
          </TabsTrigger>
          <TabsTrigger value="assets">
            <ImagePlus className="size-3.5" /> Assets
          </TabsTrigger>
          <TabsTrigger value="publish">
            <FolderGit2 className="size-3.5" /> Publish
          </TabsTrigger>
        </TabsList>
      </div>

      <div className="min-h-0 flex-1 overflow-y-auto p-4">
        <TabsContent value="preview">
          <HtmlPreview documentId={documentId} brandingId={brandingId} />
        </TabsContent>
        <TabsContent value="references">
          <ReferencesPanel documentId={documentId} />
        </TabsContent>
        <TabsContent value="sources">
          <DocumentSources documentId={documentId} />
        </TabsContent>
        <TabsContent value="assets">
          <AssetsLibrary />
        </TabsContent>
        <TabsContent value="publish">
          {document ? (
            <DocumentRepo document={document} />
          ) : (
            <Skeleton className="h-40 w-full" />
          )}
        </TabsContent>
      </div>
    </Tabs>
  );
}

/** Live HTML preview from `GET /documents/:id/render` (body + merged references,
 *  brand CSS + logo applied). The **brand previews live** — the current picker
 *  value is passed as a `?branding_id=` override so switching brands re-renders
 *  the actual document immediately. Page content still reflects the last *saved*
 *  version (it is rendered server-side from saved pages). */
function HtmlPreview({
  documentId,
  brandingId,
}: {
  documentId: string;
  brandingId: string | null;
}) {
  const { data, isLoading, error, refetch, isFetching } = useQuery({
    queryKey: ["doc-render", documentId, brandingId],
    queryFn: ({ signal }) => renderDocumentHtml(documentId, brandingId, signal),
  });

  return (
    <div className="space-y-2">
      <div className="flex items-center justify-between">
        <p className="text-[11px] text-muted-foreground">
          Brand previews live; page content reflects the last saved version.
        </p>
        <Button size="sm" variant="ghost" onClick={() => refetch()} disabled={isFetching}>
          <RefreshCw className={isFetching ? "animate-spin" : ""} /> Refresh
        </Button>
      </div>
      {isLoading ? (
        <Skeleton className="h-96 w-full" />
      ) : error ? (
        <p className="rounded-md border border-status-blocked/30 bg-status-blocked/10 px-3 py-2 text-xs text-status-blocked">
          {error instanceof ApiError ? error.message : "Could not render the preview."}
        </p>
      ) : (
        <iframe
          title="Document preview"
          srcDoc={data}
          className="h-[70vh] w-full rounded-md border border-border bg-white"
        />
      )}
    </div>
  );
}

/** Attach/detach reusable reference pages, which the renderer merges (in attach
 *  order) after the body. */
function ReferencesPanel({ documentId }: { documentId: string }) {
  const { data: attached, isLoading } = useReferences(documentId);
  const { data: docs } = useDocuments();
  const add = useAddReference();
  const remove = useRemoveReference();

  // Candidate reference pages: every `reference` document except this one.
  const references = useMemo(
    () => (docs ?? []).filter((d) => d.kind === "reference" && d.id !== documentId),
    [docs, documentId],
  );
  const attachedIds = new Set((attached ?? []).map((a) => a.thing_id));
  const titleOf = (refId: string) =>
    (docs ?? []).find((d) => d.id === refId)?.title ?? refId;

  const available = references.filter((r) => !attachedIds.has(r.id));

  return (
    <div className="space-y-3">
      <p className="text-[11px] text-muted-foreground">
        Reference pages (e.g. Terms &amp; Conditions) are merged into this document's rendered
        output, in attach order.
      </p>

      {add.error && (
        <p className="text-[11px] text-status-blocked">
          {add.error instanceof ApiError ? add.error.message : "Could not attach the reference."}
        </p>
      )}

      {isLoading ? (
        <Skeleton className="h-16 w-full" />
      ) : attached && attached.length > 0 ? (
        <ul className="divide-y divide-border rounded-md border border-border">
          {attached.map((a) => (
            <li key={a.id} className="flex items-center gap-2 px-3 py-2">
              <FileText className="size-4 shrink-0 text-accent" />
              <span className="min-w-0 flex-1 truncate text-sm">{titleOf(a.thing_id)}</span>
              <span className="shrink-0 font-mono text-[10px] text-muted-foreground">
                {a.thing_id}
              </span>
              <Button
                size="sm"
                variant="ghost"
                className="text-status-blocked"
                onClick={() => remove.mutate({ id: documentId, refId: a.thing_id })}
                disabled={remove.isPending}
                title="Un-merge"
              >
                <X className="size-3.5" />
              </Button>
            </li>
          ))}
        </ul>
      ) : (
        <p className="rounded-md border border-dashed border-border px-3 py-4 text-center text-xs text-muted-foreground">
          No reference pages merged in yet.
        </p>
      )}

      {/* Attach picker. */}
      <div className="space-y-2 rounded-md border border-border bg-surface-2/40 p-3">
        <p className="flex items-center gap-1.5 text-xs font-medium">
          <Plus className="size-3.5" /> Merge a reference page
        </p>
        {available.length === 0 ? (
          <p className="text-[11px] text-muted-foreground">
            No reference pages available. Create a document with kind “Reference page” first.
          </p>
        ) : (
          <div className="flex flex-wrap gap-1.5">
            {available.map((r) => (
              <Button
                key={r.id}
                size="sm"
                variant="secondary"
                onClick={() => add.mutate({ id: documentId, referenceId: r.id })}
                disabled={add.isPending}
              >
                <Plus className="size-3" /> {r.title}
              </Button>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}

function Field({
  label,
  hint,
  children,
}: {
  label: string;
  hint?: string;
  children: React.ReactNode;
}) {
  return (
    <label className="block space-y-1">
      <span className="text-xs font-medium">{label}</span>
      {children}
      {hint && <span className="block text-[10px] text-muted-foreground">{hint}</span>}
    </label>
  );
}
