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
import { PageEditor } from "@/components/ui/page-editor";
import { ApiError } from "@/lib/api/client";
import {
  useCreatePage,
  useDeletePage,
  usePages,
  useUpdatePage,
} from "@/lib/hooks/use-documents";
import { AssetsLibrary } from "./assets-library";
import { PageOrganizer } from "./page-organizer";
import type { Asset } from "@/types/asset";
import type { Page } from "@/types/document";

/** The pages editor: a document is a *book*, so its pages are stacked vertically
 *  and edited in place (Google-Docs style) — scroll down through them, each with
 *  its own title + markdown editor, reorder, and delete. Pages render in order
 *  and each is a page-break boundary in the exported PDF. */
/** A page's locally-edited fields, held in the parent so one "Save all" button
 *  can persist every dirty page at once. `rev` is the saved revision this draft
 *  was seeded from, so we only re-seed when the server's revision moves on. */
type PageDraftState = { title: string; body: string; page_break: boolean; rev: string };

function draftOf(p: Page): PageDraftState {
  return { title: p.title, body: p.body, page_break: p.page_break, rev: p.updated_at };
}

function isDirty(draft: PageDraftState, page: Page): boolean {
  return (
    draft.title !== page.title ||
    draft.body !== page.body ||
    draft.page_break !== page.page_break
  );
}

export function DocumentPages({ documentId }: { documentId: string }) {
  const { data: pages, isLoading } = usePages(documentId);
  const createPage = useCreatePage();
  const update = useUpdatePage();

  // Lift every page's edits up here, keyed by page id, so a single "Save all"
  // button can persist them together. A draft re-seeds from the server only when
  // that page's saved revision (`updated_at`) changes — after a save or reorder —
  // so in-progress typing on other pages is preserved across refetches.
  const [drafts, setDrafts] = useState<Record<string, PageDraftState>>({});
  useEffect(() => {
    if (!pages) return;
    setDrafts((prev) => {
      const next: Record<string, PageDraftState> = {};
      for (const p of pages) {
        const existing = prev[p.id];
        next[p.id] = existing && existing.rev === p.updated_at ? existing : draftOf(p);
      }
      return next;
    });
  }, [pages]);

  const setDraft = (id: string, patch: Partial<PageDraftState>) =>
    setDrafts((d) => ({ ...d, [id]: { ...d[id], ...patch } }));

  const dirtyPages = (pages ?? []).filter((p) => drafts[p.id] && isDirty(drafts[p.id], p));

  function addPage() {
    createPage.mutate({
      id: documentId,
      draft: { title: `Page ${(pages?.length ?? 0) + 1}`, body: "" },
    });
  }

  function saveAll() {
    for (const page of dirtyPages) {
      const d = drafts[page.id];
      update.mutate({
        id: documentId,
        pid: page.id,
        draft: { title: d.title, body: d.body, page_break: d.page_break },
      });
    }
  }

  if (isLoading) return <Skeleton className="h-64 w-full" />;

  // The same Add / Organize / Save-all row is rendered above and below the pages
  // so the actions are reachable without scrolling a long stack of pages.
  const actions = (
    <div className="flex gap-2">
      <Button
        variant="secondary"
        className="flex-1"
        onClick={addPage}
        disabled={createPage.isPending}
      >
        <Plus className="size-4" /> Add page
      </Button>
      <PageOrganizer documentId={documentId} pages={pages ?? []} />
      <Button
        className="flex-1"
        onClick={saveAll}
        disabled={dirtyPages.length === 0 || update.isPending}
      >
        <Save className="size-4" />
        {dirtyPages.length > 0 ? `Save all (${dirtyPages.length})` : "Save all"}
      </Button>
    </div>
  );

  return (
    <div className="space-y-3">
      {pages && pages.length > 0 && actions}

      {pages && pages.length > 0 ? (
        pages.map((page, i) => (
          <PageCard
            key={page.id}
            documentId={documentId}
            pages={pages}
            index={i}
            draft={drafts[page.id] ?? draftOf(page)}
            onChange={(patch) => setDraft(page.id, patch)}
          />
        ))
      ) : (
        <p className="rounded-md border border-dashed border-border px-3 py-8 text-center text-xs text-muted-foreground">
          No pages yet. Add the first page to start writing.
        </p>
      )}

      {actions}
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

/** One page in the stack: its title + markdown editor, reorder, and delete. Its
 *  edits live in the parent (a controlled `draft`) so the single "Save all"
 *  button can persist every page at once. */
function PageCard({
  documentId,
  pages,
  index,
  draft,
  onChange,
}: {
  documentId: string;
  pages: Page[];
  index: number;
  draft: PageDraftState;
  onChange: (patch: Partial<PageDraftState>) => void;
}) {
  const page = pages[index];
  const total = pages.length;
  const update = useUpdatePage();
  const del = useDeletePage();

  const dirty = isDirty(draft, page);

  function move(dir: -1 | 1) {
    // Carry the current (possibly edited) content along with the new position so
    // a reorder never discards unsaved edits.
    update.mutate({
      id: documentId,
      pid: page.id,
      draft: {
        title: draft.title,
        body: draft.body,
        page_break: draft.page_break,
        position: movePosition(pages, index, dir),
      },
    });
  }

  function insertAsset(asset: Asset) {
    const snippet = assetMarkdown(asset);
    onChange({ body: draft.body ? `${draft.body}\n\n${snippet}` : snippet });
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
          onChange={(e) => onChange({ title: e.target.value })}
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
        <PageEditor
          value={draft.body}
          onChange={(b) => onChange({ body: b })}
          placeholder={"Write this page…"}
        />
        <div className="mt-2 flex items-center gap-3">
          <label
            className="flex cursor-pointer items-center gap-1.5 text-[11px] text-muted-foreground"
            title="Keep this page in the export even when it has no content (a blank spacer page)."
          >
            <input
              type="checkbox"
              checked={draft.page_break}
              onChange={(e) => onChange({ page_break: e.target.checked })}
              className="size-3.5 accent-accent"
            />
            Page break (render even if empty)
          </label>
          {dirty && (
            <span className="ml-auto text-[10px] text-muted-foreground">Unsaved changes</span>
          )}
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
