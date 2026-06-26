import { useEffect, useState } from "react";
import { GripVertical, ListOrdered, Plus, Trash2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogTrigger,
} from "@/components/ui/dialog";
import { ApiError } from "@/lib/api/client";
import { useCreatePage, useDeletePage, useUpdatePage } from "@/lib/hooks/use-documents";
import { cn } from "@/lib/utils/cn";
import type { Page } from "@/types/document";

/** A pop-out "organize" view for a document's pages: drag the cards to reorder,
 *  add a page, or delete one — all in a single compact overview that's easier to
 *  rearrange than scrolling the full inline editor. Reordering operates on the
 *  *saved* page order (it carries each page's saved content), so it's a
 *  structural tool, distinct from the in-place content editing on the main page.
 *
 *  Drag-and-drop is native HTML5 (no extra dependency): we keep a local order
 *  while dragging for a live preview, and on drop persist the moved page to a
 *  fractional `position` between its new neighbours — mirroring the store's
 *  `position_between`. */
export function PageOrganizer({
  documentId,
  pages,
}: {
  documentId: string;
  pages: Page[];
}) {
  const [open, setOpen] = useState(false);
  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogTrigger asChild>
        <Button variant="secondary" disabled={pages.length === 0} title="Reorder, add or delete pages">
          <ListOrdered className="size-4" /> Organize
        </Button>
      </DialogTrigger>
      <DialogContent
        title="Organize pages"
        description="Drag a page by its handle to reorder. Add or delete pages here too."
        className="max-w-lg"
      >
        <OrganizerBody documentId={documentId} pages={pages} />
        <div className="mt-4 flex justify-end">
          <DialogClose asChild>
            <Button variant="ghost" size="sm">
              Done
            </Button>
          </DialogClose>
        </div>
      </DialogContent>
    </Dialog>
  );
}

function OrganizerBody({ documentId, pages }: { documentId: string; pages: Page[] }) {
  const createPage = useCreatePage();
  const update = useUpdatePage();
  const del = useDeletePage();

  // A local ordering of page ids drives the live drag preview. It re-seeds from
  // the server whenever the page set or its order changes (after a persisted
  // reorder, add, or delete), but stays put mid-drag.
  const [order, setOrder] = useState<string[]>(() => pages.map((p) => p.id));
  const [dragId, setDragId] = useState<string | null>(null);
  const [overId, setOverId] = useState<string | null>(null);

  const serverOrder = pages.map((p) => p.id).join(",");
  useEffect(() => {
    setOrder(pages.map((p) => p.id));
  }, [serverOrder]); // eslint-disable-line react-hooks/exhaustive-deps

  const byId = new Map(pages.map((p) => [p.id, p]));

  function reorderTo(targetId: string) {
    if (!dragId || dragId === targetId) return;
    setOrder((cur) => {
      const next = cur.filter((id) => id !== dragId);
      const at = next.indexOf(targetId);
      next.splice(at, 0, dragId);
      return next;
    });
  }

  function drop() {
    const moved = dragId;
    setDragId(null);
    setOverId(null);
    if (!moved) return;
    const page = byId.get(moved);
    if (!page) return;
    // Persist only if the moved page actually changed neighbours.
    const i = order.indexOf(moved);
    const before = order[i - 1] ? byId.get(order[i - 1]) : undefined;
    const after = order[i + 1] ? byId.get(order[i + 1]) : undefined;
    const position = between(before?.position, after?.position);
    if (position === page.position) return;
    update.mutate({
      id: documentId,
      pid: moved,
      draft: {
        title: page.title,
        body: page.body,
        page_break: page.page_break,
        position,
      },
    });
  }

  function addPage() {
    createPage.mutate({
      id: documentId,
      draft: { title: `Page ${pages.length + 1}`, body: "" },
    });
  }

  const error = update.error ?? createPage.error ?? del.error;

  return (
    <div className="space-y-3">
      <ul className="space-y-1.5" onDragOver={(e) => e.preventDefault()}>
        {order.map((id, i) => {
          const page = byId.get(id);
          if (!page) return null;
          return (
            <OrganizerRow
              key={id}
              index={i}
              page={page}
              dragging={dragId === id}
              over={overId === id && dragId != null && dragId !== id}
              onDragStart={() => setDragId(id)}
              onDragEnter={() => {
                setOverId(id);
                reorderTo(id);
              }}
              onDrop={drop}
              onRename={(title) =>
                update.mutate({
                  id: documentId,
                  pid: id,
                  draft: {
                    title,
                    body: page.body,
                    page_break: page.page_break,
                    position: page.position,
                  },
                })
              }
              onDelete={() => del.mutate({ id: documentId, pid: id })}
              deleting={del.isPending}
            />
          );
        })}
      </ul>

      <Button
        variant="secondary"
        className="w-full"
        onClick={addPage}
        disabled={createPage.isPending}
      >
        <Plus className="size-4" /> Add page
      </Button>

      {error && (
        <p className="text-[11px] text-status-blocked">
          {error instanceof ApiError ? error.message : "Could not save the change."}
        </p>
      )}
    </div>
  );
}

/** One draggable page row: a grip handle, position number, an inline-editable
 *  title (saves on blur/Enter when changed), an "empty" badge, and delete. The
 *  title is a local draft so typing stays smooth; it re-seeds when the saved
 *  title changes. */
function OrganizerRow({
  index,
  page,
  dragging,
  over,
  onDragStart,
  onDragEnter,
  onDrop,
  onRename,
  onDelete,
  deleting,
}: {
  index: number;
  page: Page;
  dragging: boolean;
  over: boolean;
  onDragStart: () => void;
  onDragEnter: () => void;
  onDrop: () => void;
  onRename: (title: string) => void;
  onDelete: () => void;
  deleting: boolean;
}) {
  const [title, setTitle] = useState(page.title);
  useEffect(() => setTitle(page.title), [page.title]);

  function commit() {
    const next = title.trim();
    if (next && next !== page.title) onRename(next);
    else if (!next) setTitle(page.title); // don't allow an empty title
  }

  return (
    <li
      draggable
      onDragStart={onDragStart}
      onDragEnter={onDragEnter}
      onDragEnd={onDrop}
      onDrop={(e) => {
        e.preventDefault();
        onDrop();
      }}
      className={cn(
        "flex items-center gap-2 rounded-md border bg-surface-2/40 px-2 py-1.5 transition-colors",
        dragging
          ? "border-accent/60 opacity-50"
          : over
            ? "border-accent/50 bg-accent/10"
            : "border-border",
      )}
    >
      {/* Only the handle initiates a drag, so the title input stays selectable. */}
      <GripVertical
        className="size-4 shrink-0 cursor-grab text-muted-foreground active:cursor-grabbing"
        aria-label="Drag to reorder"
      />
      <span className="shrink-0 rounded bg-surface-2 px-1.5 py-0.5 font-mono text-[10px] text-muted-foreground">
        {index + 1}
      </span>
      <Input
        value={title}
        onChange={(e) => setTitle(e.target.value)}
        onBlur={commit}
        onKeyDown={(e) => {
          if (e.key === "Enter") e.currentTarget.blur();
          if (e.key === "Escape") {
            setTitle(page.title);
            e.currentTarget.blur();
          }
        }}
        placeholder="Page title"
        className="h-7 min-w-0 flex-1 border-0 bg-transparent px-1 text-sm focus-visible:ring-0"
        title="Rename page"
      />
      {!page.body.trim() && (
        <span className="shrink-0 rounded bg-surface-2 px-1.5 py-0.5 text-[10px] text-muted-foreground">
          empty
        </span>
      )}
      <Button
        size="sm"
        variant="ghost"
        className="size-6 shrink-0 p-0 text-status-blocked"
        title="Delete page"
        disabled={deleting}
        onClick={onDelete}
      >
        <Trash2 className="size-3.5" />
      </Button>
    </li>
  );
}

/** A fractional position landing between two neighbours (midpoint), mirroring the
 *  store's `position_between`. Shared shape with the inline editor's reorder. */
function between(before?: number, after?: number): number {
  if (before != null && after != null) return (before + after) / 2;
  if (before != null) return before + 1;
  if (after != null) return after - 1;
  return 1;
}
