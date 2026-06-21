import { useMemo, useState } from "react";
import { GripVertical, ListOrdered, AlertTriangle } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import {
  Dialog,
  DialogContent,
  DialogTrigger,
  DialogClose,
} from "@/components/ui/dialog";
import { StatusBadge } from "@/components/ui/status-badge";
import { cn } from "@/lib/utils/cn";
import { ApiError } from "@/lib/api/client";
import { useUpdateTask } from "@/lib/hooks/use-tasks";
import { executionOrder, linearisedDeps } from "./reorder-deps";
import type { Task } from "@/types/task";

/** Reorder a workflow's tasks by drag-and-drop. Execution order is *derived* from
 *  the dependency graph (there's no stored position), so reordering rewrites the
 *  real lever: deps. Saving makes each task run after the one directly above it
 *  (a sequential chain), keeping existing upstream edges and dropping any that
 *  would now point forward. This linearises the workflow — surfaced plainly so
 *  it's never a surprise. */
export function ReorderDialog({
  tasks,
  trigger,
}: {
  tasks: Task[];
  trigger?: React.ReactNode;
}) {
  const [open, setOpen] = useState(false);

  return (
    <Dialog
      open={open}
      onOpenChange={(o) => {
        setOpen(o);
      }}
    >
      <DialogTrigger asChild>
        {trigger ?? (
          <Button size="sm" variant="ghost">
            <ListOrdered /> Reorder
          </Button>
        )}
      </DialogTrigger>
      <DialogContent
        title="Reorder tasks"
        description="Drag to set execution order. Each task runs after the one above it — this rewrites dependencies, so it linearises the workflow."
        className="max-h-[85vh] overflow-y-auto"
      >
        {/* Remount the body each time the dialog opens so it picks up the latest
            order and clears any half-finished drag from a previous session. */}
        {open && <ReorderBody tasks={tasks} onDone={() => setOpen(false)} />}
      </DialogContent>
    </Dialog>
  );
}

function ReorderBody({ tasks, onDone }: { tasks: Task[]; onDone: () => void }) {
  const byId = useMemo(() => new Map(tasks.map((t) => [t.id, t])), [tasks]);
  // Initial order = the current derived execution order (same as the Tasks tab).
  const initial = useMemo(() => executionOrder(tasks).map((o) => o.task.id), [tasks]);
  const [order, setOrder] = useState<string[]>(initial);
  const [draggingId, setDraggingId] = useState<string | null>(null);
  const [overId, setOverId] = useState<string | null>(null);

  const update = useUpdateTask();
  const [error, setError] = useState<string | null>(null);

  // What would actually change on save — drives the disabled state + summary.
  const pending = useMemo(() => linearisedDeps(order, byId), [order, byId]);
  const dirty = pending.size > 0;

  function move(dragId: string, targetId: string) {
    if (dragId === targetId) return;
    setOrder((cur) => {
      const next = cur.filter((id) => id !== dragId);
      const at = next.indexOf(targetId);
      next.splice(at, 0, dragId);
      return next;
    });
  }

  async function save() {
    setError(null);
    const edits = [...pending.entries()];
    try {
      // Sequential so a mid-way failure leaves a coherent partial state and the
      // error surfaces (the dep diff is small — at most one chain edge per task).
      for (const [id, deps] of edits) {
        const t = byId.get(id)!;
        await update.mutateAsync({
          id,
          draft: {
            title: t.title,
            spec: t.spec,
            deps,
            owns: t.owns,
            tool: t.tool,
            model: t.model,
            effort: t.effort,
            worktree_mode: t.worktree_mode,
          },
        });
      }
      onDone();
    } catch (e) {
      setError(
        e instanceof ApiError ? e.message : "Couldn't save the new order.",
      );
    }
  }

  return (
    <div className="space-y-3">
      <ol className="space-y-1.5">
        {order.map((id, i) => {
          const task = byId.get(id);
          if (!task) return null;
          const isDragging = draggingId === id;
          const isOver = overId === id && draggingId !== id;
          return (
            <li
              key={id}
              draggable
              onDragStart={(e) => {
                setDraggingId(id);
                e.dataTransfer.effectAllowed = "move";
                e.dataTransfer.setData("text/plain", id);
              }}
              onDragEnd={() => {
                setDraggingId(null);
                setOverId(null);
              }}
              onDragOver={(e) => {
                e.preventDefault();
                e.dataTransfer.dropEffect = "move";
                if (overId !== id) setOverId(id);
              }}
              onDrop={(e) => {
                e.preventDefault();
                const dragId = e.dataTransfer.getData("text/plain") || draggingId;
                if (dragId) move(dragId, id);
                setDraggingId(null);
                setOverId(null);
              }}
              className={cn(
                "flex items-center gap-2 rounded-md border border-border bg-surface p-2.5 transition-colors",
                isDragging && "opacity-40",
                isOver && "border-accent/60 bg-accent-soft/20",
              )}
            >
              <GripVertical className="size-4 shrink-0 cursor-grab text-muted-foreground active:cursor-grabbing" />
              <Badge variant="accent" className="shrink-0 tabular-nums">
                {i + 1}
              </Badge>
              <div className="min-w-0 flex-1">
                <div className="flex items-center gap-2">
                  <span className="truncate font-mono text-sm font-medium">{task.id}</span>
                  <StatusBadge status={task.status} />
                </div>
                <p className="truncate text-xs text-muted-foreground">{task.title}</p>
              </div>
            </li>
          );
        })}
      </ol>

      <div className="flex items-start gap-2 rounded-md border border-status-pending/40 bg-status-pending/10 px-3 py-2 text-[11px] text-muted-foreground">
        <AlertTriangle className="mt-0.5 size-3.5 shrink-0 text-status-pending" />
        <span>
          Saving makes the workflow run top-to-bottom: each task is set to depend
          on the one above it. Existing parallel branches are collapsed into this
          single chain.
        </span>
      </div>

      {error && (
        <p className="rounded-md border border-status-blocked/30 bg-status-blocked/10 px-3 py-2 text-xs text-status-blocked">
          {error}
        </p>
      )}

      <div className="flex items-center justify-between gap-2">
        <span className="text-[11px] text-muted-foreground">
          {dirty
            ? `${pending.size} task${pending.size > 1 ? "s" : ""} will change`
            : "No changes"}
        </span>
        <div className="flex gap-2">
          <DialogClose asChild>
            <Button variant="ghost" size="sm">
              Cancel
            </Button>
          </DialogClose>
          <Button
            size="sm"
            onClick={save}
            disabled={!dirty || update.isPending}
          >
            {update.isPending ? "Saving…" : "Save order"}
          </Button>
        </div>
      </div>
    </div>
  );
}
