import { useEffect, useState } from "react";
import {
  ChevronDown,
  ChevronUp,
  FileText,
  ImagePlus,
  Plus,
  Save,
  Trash2,
} from "lucide-react";
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

/** The pages editor: a document is a *book*, so its content is authored one page
 *  at a time. A left rail lists the pages (the table of contents) with reorder +
 *  delete; selecting one opens its title + markdown editor. Pages render in order
 *  and each is a page-break boundary in the exported PDF. */
export function DocumentPages({ documentId }: { documentId: string }) {
  const { data: pages, isLoading } = usePages(documentId);
  const createPage = useCreatePage();
  const updatePage = useUpdatePage();
  const deletePage = useDeletePage();

  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [draft, setDraft] = useState({ title: "", body: "" });

  const selected = pages?.find((p) => p.id === selectedId) ?? null;

  // Auto-select the first page once the list loads (or after a delete).
  useEffect(() => {
    if (pages && pages.length > 0 && !pages.some((p) => p.id === selectedId)) {
      setSelectedId(pages[0].id);
    }
    if (pages && pages.length === 0) setSelectedId(null);
  }, [pages, selectedId]);

  // Sync the local draft to the selected page (keyed on its saved revision so an
  // external save/refresh re-seeds the editor, but local typing is preserved).
  useEffect(() => {
    if (selected) setDraft({ title: selected.title, body: selected.body });
  }, [selected?.id, selected?.updated_at]);

  const dirty =
    !!selected && (draft.title !== selected.title || draft.body !== selected.body);

  function addPage() {
    createPage.mutate(
      { id: documentId, draft: { title: `Page ${(pages?.length ?? 0) + 1}`, body: "" } },
      { onSuccess: (p) => setSelectedId(p.id) },
    );
  }

  function saveSelected() {
    if (!selected || !dirty) return;
    updatePage.mutate({
      id: documentId,
      pid: selected.id,
      draft: { title: draft.title, body: draft.body },
    });
  }

  function move(index: number, dir: -1 | 1) {
    if (!pages) return;
    const page = pages[index];
    updatePage.mutate({
      id: documentId,
      pid: page.id,
      // Keep the page's saved content; only its position changes.
      draft: { title: page.title, body: page.body, position: movePosition(pages, index, dir) },
    });
  }

  function insertAsset(asset: Asset) {
    const snippet = assetMarkdown(asset);
    setDraft((d) => ({ ...d, body: d.body ? `${d.body}\n\n${snippet}` : snippet }));
  }

  if (isLoading) return <Skeleton className="h-64 w-full" />;

  const err = createPage.error || updatePage.error || deletePage.error;

  return (
    <div className="space-y-3">
      {/* Page rail (table of contents). */}
      <div className="space-y-2 rounded-md border border-border">
        <div className="flex items-center justify-between border-b border-border px-3 py-2">
          <span className="text-xs font-medium">Pages ({pages?.length ?? 0})</span>
          <Button size="sm" variant="ghost" onClick={addPage} disabled={createPage.isPending}>
            <Plus className="size-3.5" /> Add page
          </Button>
        </div>
        {pages && pages.length > 0 ? (
          <ul className="divide-y divide-border">
            {pages.map((p, i) => (
              <li
                key={p.id}
                className={`flex items-center gap-1.5 px-2 py-1.5 ${
                  p.id === selectedId ? "bg-accent/10" : ""
                }`}
              >
                <button
                  className="flex min-w-0 flex-1 items-center gap-2 text-left"
                  onClick={() => setSelectedId(p.id)}
                >
                  <FileText className="size-3.5 shrink-0 text-accent" />
                  <span className="min-w-0 flex-1 truncate text-sm">
                    {p.title || <span className="text-muted-foreground">Untitled page</span>}
                  </span>
                </button>
                <Button
                  size="sm"
                  variant="ghost"
                  className="size-6 p-0"
                  title="Move up"
                  disabled={i === 0 || updatePage.isPending}
                  onClick={() => move(i, -1)}
                >
                  <ChevronUp className="size-3.5" />
                </Button>
                <Button
                  size="sm"
                  variant="ghost"
                  className="size-6 p-0"
                  title="Move down"
                  disabled={i === pages.length - 1 || updatePage.isPending}
                  onClick={() => move(i, 1)}
                >
                  <ChevronDown className="size-3.5" />
                </Button>
                <Button
                  size="sm"
                  variant="ghost"
                  className="size-6 p-0 text-status-blocked"
                  title="Delete page"
                  disabled={deletePage.isPending}
                  onClick={() => deletePage.mutate({ id: documentId, pid: p.id })}
                >
                  <Trash2 className="size-3.5" />
                </Button>
              </li>
            ))}
          </ul>
        ) : (
          <p className="px-3 py-6 text-center text-xs text-muted-foreground">
            No pages yet. Add the first page to start writing.
          </p>
        )}
      </div>

      {err && (
        <p className="rounded-md border border-status-blocked/30 bg-status-blocked/10 px-3 py-2 text-xs text-status-blocked">
          {err instanceof ApiError ? err.message : "Something went wrong."}
        </p>
      )}

      {/* Selected-page editor. */}
      {selected && (
        <div className="space-y-2 rounded-md border border-border p-3">
          <div className="flex items-center gap-2">
            <Input
              value={draft.title}
              onChange={(e) => setDraft({ ...draft, title: e.target.value })}
              placeholder="Page title"
              className="h-8"
            />
            <Button size="sm" onClick={saveSelected} disabled={!dirty || updatePage.isPending}>
              <Save className="size-3.5" /> Save page
            </Button>
          </div>
          <div className="mb-1 flex items-center justify-end">
            <InsertAssetButton onInsert={insertAsset} />
          </div>
          <MarkdownEditor
            value={draft.body}
            onChange={(b) => setDraft({ ...draft, body: b })}
            placeholder={"# Heading\n\nWrite this page in markdown…"}
          />
        </div>
      )}
    </div>
  );
}

/** A fractional position that moves the page at `index` one slot in `dir`,
 *  landing it between its new neighbours (midpoint), mirroring the store's
 *  `position_between`. */
function movePosition(pages: Page[], index: number, dir: -1 | 1): number {
  if (dir === -1) {
    // Land above the page currently above it.
    return between(pages[index - 2]?.position, pages[index - 1].position);
  }
  // Land below the page currently below it.
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
        <Button size="sm" variant="ghost" className="h-6 px-2 text-[11px]">
          <ImagePlus className="size-3" /> Insert asset
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
