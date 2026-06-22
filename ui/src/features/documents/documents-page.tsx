import { useState } from "react";
import {
  BookText,
  Files,
  Plus,
  ServerCrash,
} from "lucide-react";
import { Topbar } from "@/components/layout/topbar";
import { Button } from "@/components/ui/button";
import { EmptyState } from "@/components/ui/empty-state";
import { Skeleton } from "@/components/ui/skeleton";
import { Card } from "@/components/ui/card";
import { Tabs, TabsList, TabsTrigger, TabsContent } from "@/components/ui/tabs";
import { ApiError } from "@/lib/api/client";
import { shortTime } from "@/lib/utils/platform";
import { useDocuments } from "@/lib/hooks/use-documents";
import { BrandSwatch } from "@/components/brand-picker";
import { useBrandingList } from "@/lib/hooks/use-branding";
import type { DocKind, Document } from "@/types/document";
import { DocumentEditor } from "./document-editor";

/** The Documents view: a list of authored documents and a separate Reference-
 *  pages view (reusable pages like Terms & Conditions, which are documents with
 *  `kind = reference`). Selecting one — or "New" — opens the full-page editor. */
export function DocumentsPage() {
  const { data: documents, isLoading, error } = useDocuments();
  // `null` = list; `{ id }` = edit; `{ id: undefined, kind }` = author.
  const [open, setOpen] = useState<{ id?: string; kind?: DocKind } | null>(null);

  if (open) {
    return (
      <DocumentEditor
        documentId={open.id}
        initialKind={open.kind ?? "document"}
        onBack={() => setOpen(null)}
      />
    );
  }

  const docs = (documents ?? []).filter((d) => d.kind !== "reference");
  const refs = (documents ?? []).filter((d) => d.kind === "reference");

  const subtitle = documents
    ? `${docs.length} document${docs.length === 1 ? "" : "s"} · ${refs.length} reference${refs.length === 1 ? "" : "s"}`
    : "Loading…";

  return (
    <div className="flex h-full flex-col">
      <Topbar title="Documents" subtitle={subtitle} />
      <div className="min-h-0 flex-1 overflow-auto p-5">
        {error ? (
          <EmptyState
            icon={ServerCrash}
            title="Can't load documents"
            description={error instanceof ApiError ? error.message : "Unexpected error"}
          />
        ) : (
          <Tabs defaultValue="documents" className="space-y-4">
            <TabsList>
              <TabsTrigger value="documents">
                <BookText className="size-3.5" /> Documents
              </TabsTrigger>
              <TabsTrigger value="references">
                <Files className="size-3.5" /> Reference pages
              </TabsTrigger>
            </TabsList>

            <TabsContent value="documents">
              <DocList
                docs={docs}
                loading={isLoading && !documents}
                kind="document"
                emptyTitle="No documents yet"
                emptyDescription="Author a branded markdown document — a quote, a report, a letter."
                onOpen={(id) => setOpen({ id })}
                onNew={() => setOpen({ kind: "document" })}
              />
            </TabsContent>

            <TabsContent value="references">
              <DocList
                docs={refs}
                loading={isLoading && !documents}
                kind="reference"
                emptyTitle="No reference pages yet"
                emptyDescription="Reference pages (e.g. Terms & Conditions) are reusable pages merged into other documents' rendered output."
                onOpen={(id) => setOpen({ id })}
                onNew={() => setOpen({ kind: "reference" })}
              />
            </TabsContent>
          </Tabs>
        )}
      </div>
    </div>
  );
}

function DocList({
  docs,
  loading,
  kind,
  emptyTitle,
  emptyDescription,
  onOpen,
  onNew,
}: {
  docs: Document[];
  loading: boolean;
  kind: DocKind;
  emptyTitle: string;
  emptyDescription: string;
  onOpen: (id: string) => void;
  onNew: () => void;
}) {
  const newButton = (
    <Button size="sm" onClick={onNew}>
      <Plus /> {kind === "reference" ? "New reference" : "New document"}
    </Button>
  );

  if (loading) {
    return (
      <div className="grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-3">
        {Array.from({ length: 3 }).map((_, i) => (
          <Skeleton key={i} className="h-28 w-full" />
        ))}
      </div>
    );
  }

  if (docs.length === 0) {
    return (
      <EmptyState
        icon={kind === "reference" ? Files : BookText}
        title={emptyTitle}
        description={emptyDescription}
        action={newButton}
      />
    );
  }

  return (
    <div className="space-y-3">
      <div className="flex justify-end">{newButton}</div>
      <div className="grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-3">
        {docs.map((d) => (
          <DocCard key={d.id} doc={d} onOpen={() => onOpen(d.id)} />
        ))}
      </div>
    </div>
  );
}

function DocCard({ doc, onOpen }: { doc: Document; onOpen: () => void }) {
  const { data: brands } = useBrandingList();
  const brand = brands?.find((b) => b.id === doc.branding_id) ?? null;

  return (
    <Card
      className="cursor-pointer p-4 transition-colors hover:border-border-strong"
      onClick={onOpen}
    >
      <div className="flex items-start justify-between gap-2">
        <p className="min-w-0 flex-1 truncate text-sm font-medium">{doc.title}</p>
        {doc.repo?.pr_url && (
          <span className="shrink-0 rounded-full border border-accent/40 px-1.5 py-px text-[10px] text-accent">
            PR
          </span>
        )}
      </div>
      <p className="mt-0.5 truncate font-mono text-[10px] text-muted-foreground">{doc.id}</p>
      <p className="mt-2 line-clamp-2 text-xs text-muted-foreground">
        {doc.body.trim() ? doc.body.slice(0, 160) : "Empty document."}
      </p>
      <div className="mt-3 flex items-center justify-between">
        {brand ? <BrandSwatch brand={brand} /> : <span className="text-[10px] text-muted-foreground">Default brand</span>}
        <span className="text-[10px] text-muted-foreground">{shortTime(doc.updated_at)}</span>
      </div>
    </Card>
  );
}
