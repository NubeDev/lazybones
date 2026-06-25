import { useEffect, useState } from "react";
import { ChevronDown, ChevronUp, ImagePlus, Plus, Save, Trash2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Skeleton } from "@/components/ui/skeleton";
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogTrigger,
} from "@/components/ui/dialog";
import { MarkdownEditor } from "@/components/ui/markdown-editor";
import { ApiError } from "@/lib/api/client";
import {
  useCreatePage,
  useDeletePage,
  usePages,
  useUpdatePage,
} from "@/lib/hooks/use-documents";
import { AssetsLibrary } from "./assets-library";
import type { Asset } from "@/types/asset";
import type { Page } from "@/types/document";

/** The pages editor: a document is a *book*, so its pages are stacked vertically
 *  and edited in place (Google-Docs style) — scroll down through them, each with
 *  its own title + markdown editor, reorder, and delete. Pages render in order
 *  and each is a page-break boundary in the exported PDF. */
export function DocumentPages({ documentId }: { documentId: string }) {
  const { data: pages, isLoading } = usePages(documentId);
  const createPage = useCreatePage();

  function addPage() {
    createPage.mutate({
      id: documentId,
      draft: { title: `Page ${(pages?.length ?? 0) + 1}`, body: "" },
    });
  }

  if (isLoading) return <Skeleton className="h-64 w-full" />;

  return (
    <div className="space-y-3">
      {pages && pages.length > 0 ? (
        pages.map((page, i) => (
          <PageCard key={page.id} documentId={documentId} pages={pages} index={i} />
        ))
      ) : (
        <p className="rounded-md border border-dashed border-border px-3 py-8 text-center text-xs text-muted-foreground">
          No pages yet. Add the first page to start writing.
        </p>
      )}

      <Button
        variant="secondary"
        className="w-full"
        onClick={addPage}
        disabled={createPage.isPending}
      >
        <Plus className="size-4" /> Add page
      </Button>
      {createPage.error && (
        <p className="text-[11px] text-status-blocked">
          {createPage.error instanceof ApiError
            ? createPage.error.message
            : "Could not add the page."}
        </p>
      )}
    </div>
  );
}

/** One page in the stack: its own title + markdown editor with a per-page Save,
 *  plus reorder and delete. Local edits are isolated to this card. */
function PageCard({
  documentId,
  pages,
  index,
}: {
  documentId: string;
  pages: Page[];
  index: number;
}) {
  const page = pages[index];
  const total = pages.length;
  const update = useUpdatePage();
  const del = useDeletePage();

  const [draft, setDraft] = useState({ title: page.title, body: page.body });
  // Re-seed from the server when this page's saved revision changes (after a
  // save or a reorder), but keep local typing between renders otherwise.
  useEffect(() => {
    setDraft({ title: page.title, body: page.body });
  }, [page.id, page.updated_at]);

  const dirty = draft.title !== page.title || draft.body !== page.body;

  function save() {
    if (!dirty) return;
    update.mutate({
      id: documentId,
      pid: page.id,
      draft: { title: draft.title, body: draft.body },
    });
  }

  function move(dir: -1 | 1) {
    // Carry the current (possibly edited) content along with the new position so
    // a reorder never discards unsaved edits.
    update.mutate({
      id: documentId,
      pid: page.id,
      draft: { title: draft.title, body: draft.body, position: movePosition(pages, index, dir) },
    });
  }

  function insertAsset(asset: Asset) {
    const snippet = assetMarkdown(asset);
    setDraft((d) => ({ ...d, body: d.body ? `${d.body}\n\n${snippet}` : snippet }));
  }

  return (
    <div className="rounded-md border border-border bg-surface-2/30">
      {/* Page header: index, title, controls. */}
      <div className="flex items-center gap-2 border-b border-border px-2 py-1.5">
        <span className="shrink-0 rounded bg-surface-2 px-1.5 py-0.5 font-mono text-[10px] text-muted-foreground">
          {index + 1}
        </span>
        <Input
          value={draft.title}
          onChange={(e) => setDraft({ ...draft, title: e.target.value })}
          placeholder="Page title"
          className="h-7 flex-1 border-0 bg-transparent px-1 text-sm font-medium focus-visible:ring-0"
        />
        <Button
          size="sm"
          variant="ghost"
          className="size-6 p-0"
          title="Move up"
          disabled={index === 0 || update.isPending}
          onClick={() => move(-1)}
        >
          <ChevronUp className="size-3.5" />
        </Button>
        <Button
          size="sm"
          variant="ghost"
          className="size-6 p-0"
          title="Move down"
          disabled={index === total - 1 || update.isPending}
          onClick={() => move(1)}
        >
          <ChevronDown className="size-3.5" />
        </Button>
        <InsertAssetButton onInsert={insertAsset} />
        <Button
          size="sm"
          variant="ghost"
          className="size-6 p-0 text-status-blocked"
          title="Delete page"
          disabled={del.isPending}
          onClick={() => del.mutate({ id: documentId, pid: page.id })}
        >
          <Trash2 className="size-3.5" />
        </Button>
      </div>

      {/* Page body. */}
      <div className="p-2">
        <MarkdownEditor
          value={draft.body}
          onChange={(b) => setDraft({ ...draft, body: b })}
          placeholder={"# Heading\n\nWrite this page in markdown…"}
        />
        <div className="mt-2 flex items-center justify-end gap-2">
          {dirty && <span className="text-[10px] text-muted-foreground">Unsaved changes</span>}
          <Button size="sm" onClick={save} disabled={!dirty || update.isPending}>
            <Save className="size-3.5" /> Save page
          </Button>
        </div>
      </div>
    </div>
  );
}

/** A fractional position that moves the page at `index` one slot in `dir`,
 *  landing it between its new neighbours (midpoint), mirroring the store's
 *  `position_between`. */
function movePosition(pages: Page[], index: number, dir: -1 | 1): number {
  if (dir === -1) {
    return between(pages[index - 2]?.position, pages[index - 1].position);
  }
  return between(pages[index + 1].position, pages[index + 2]?.position);
}

function between(before?: number, after?: number): number {
  if (before != null && after != null) return (before + after) / 2;
  if (before != null) return before + 1;
  if (after != null) return after - 1;
  return 1;
}

/** A dialog wrapping the asset library so the author can pick an image to insert
 *  into the page body. */
function InsertAssetButton({ onInsert }: { onInsert: (asset: Asset) => void }) {
  const [open, setOpen] = useState(false);
  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogTrigger asChild>
        <Button size="sm" variant="ghost" className="size-6 p-0" title="Insert asset">
          <ImagePlus className="size-3.5" />
        </Button>
      </DialogTrigger>
      <DialogContent
        title="Insert an asset"
        description="Pick an image to embed in this page, or upload a new one."
        className="max-w-2xl"
      >
        <div className="max-h-[60vh] overflow-y-auto">
          <AssetsLibrary
            onInsert={(a) => {
              onInsert(a);
              setOpen(false);
            }}
          />
        </div>
        <div className="mt-3 flex justify-end">
          <DialogClose asChild>
            <Button variant="ghost" size="sm">
              Close
            </Button>
          </DialogClose>
        </div>
      </DialogContent>
    </Dialog>
  );
}

/** A markdown image referencing an asset by its server path (the export route
 *  resolves `/assets/<id>` to the inline bytes). */
function assetMarkdown(asset: Asset): string {
  return `![${asset.filename}](/assets/${asset.id})`;
}
