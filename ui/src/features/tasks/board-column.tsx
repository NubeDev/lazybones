import { useState } from "react";
import { TaskCard } from "./task-card";
import { STATUS_META } from "@/types/status-meta";
import { cn } from "@/lib/utils/cn";
import { isPlausibleTarget } from "./drag-rules";
import type { Status, Task } from "@/types/task";

/** One lifecycle column on the board. Doubles as a drop zone: while a card is
 *  dragged, the column highlights only if it is a legal target for that card's
 *  current status (promote / block), and a drop fires `onDrop`. */
export function BoardColumn({
  status,
  tasks,
  selectedId,
  draggingId,
  draggingFrom,
  onSelect,
  onDragStart,
  onDragEnd,
  onDrop,
  blockedReason,
}: {
  status: Status;
  tasks: Task[];
  selectedId: string | null;
  draggingId: string | null;
  draggingFrom: Status | null;
  onSelect: (id: string) => void;
  onDragStart: (id: string) => void;
  onDragEnd: () => void;
  onDrop: (to: Status, droppedId: string) => void;
  /** Why this task can't be promoted yet (deps unmet), or null if it can. */
  blockedReason: (task: Task) => string | null;
}) {
  const meta = STATUS_META[status];
  const Icon = meta.icon;
  const [over, setOver] = useState(false);

  // Is this column a valid place to drop the card currently being dragged?
  const isTarget = draggingFrom !== null && isPlausibleTarget(draggingFrom, status);

  return (
    <div className="flex w-72 shrink-0 flex-col">
      <div className="mb-3 flex items-center gap-2 px-1">
        <Icon className="size-3.5" style={{ color: meta.color }} />
        <span className="text-xs font-semibold tracking-tight" style={{ color: meta.color }}>
          {meta.label}
        </span>
        <span className="rounded-full bg-muted px-1.5 text-[10px] font-medium text-muted-foreground">
          {tasks.length}
        </span>
      </div>

      <div
        onDragEnter={(e) => e.preventDefault()}
        onDragOver={(e) => {
          // A drop fires only if dragover both calls preventDefault AND leaves a
          // non-"none" dropEffect — setting dropEffect="none" silently rejects the
          // drop even after preventDefault. So always allow the drop to reach the
          // board (which is the sole arbiter of legality via dropAction); never
          // gate it on React drag state, which can lag a mid-drag refetch.
          e.preventDefault();
          e.dataTransfer.dropEffect = "move";
          if (isTarget) setOver(true);
        }}
        onDragLeave={() => setOver(false)}
        onDrop={(e) => {
          e.preventDefault();
          setOver(false);
          onDrop(status, e.dataTransfer.getData("text/plain"));
        }}
        className={cn(
          "flex flex-1 flex-col gap-2 rounded-lg border border-dashed border-border/60 bg-surface-2/40 p-2 transition-colors",
          isTarget && "border-accent/40",
          over && "border-accent bg-accent-soft/30 ring-1 ring-accent/40",
        )}
      >
        {tasks.length === 0 ? (
          <p className="px-2 py-6 text-center text-[11px] text-muted-foreground/60">
            {over
              ? `drop to ${status === "blocked" ? "block" : "promote"}`
              : "empty"}
          </p>
        ) : (
          tasks.map((t) => (
            <TaskCard
              key={t.id}
              task={t}
              selected={selectedId === t.id}
              draggable={t.status !== "done" && t.status !== "blocked"}
              dragging={draggingId === t.id}
              blockedReason={blockedReason(t)}
              onSelect={onSelect}
              onDragStart={onDragStart}
              onDragEnd={onDragEnd}
            />
          ))
        )}
      </div>
    </div>
  );
}
