import { useMemo, useState } from "react";
import { BoardColumn } from "./board-column";
import { BOARD_COLUMNS, groupByStatus } from "./group-tasks";
import { dropAction, promoteBlockedReason } from "./drag-rules";
import { BlockReasonDialog } from "./block-reason-dialog";
import { Skeleton } from "@/components/ui/skeleton";
import { EmptyState } from "@/components/ui/empty-state";
import { ServerCrash, Inbox } from "lucide-react";
import type { Status, Task } from "@/types/task";
import { ApiError } from "@/lib/api/client";
import { useReadyTask } from "@/lib/hooks/use-tasks";

/** The kanban board: one column per lifecycle status, horizontally scrolling.
 *  Cards drag between columns for the two operator moves the backend allows —
 *  promote (`pending → ready`) and block (`* → blocked`); illegal targets don't
 *  accept the drop. */
export function TaskBoard({
  tasks,
  isLoading,
  error,
  selectedId,
  onSelect,
}: {
  tasks: Task[] | undefined;
  isLoading: boolean;
  error: unknown;
  selectedId: string | null;
  onSelect: (id: string) => void;
}) {
  const ready = useReadyTask();
  // The task currently being dragged, and a pending block target awaiting a reason.
  const [draggingId, setDraggingId] = useState<string | null>(null);
  const [blockingId, setBlockingId] = useState<string | null>(null);

  const byId = useMemo(
    () => new Map((tasks ?? []).map((t) => [t.id, t])),
    [tasks],
  );

  if (isLoading && !tasks) {
    return (
      <div className="flex gap-4 overflow-hidden">
        {BOARD_COLUMNS.slice(0, 4).map((s) => (
          <div key={s} className="w-72 shrink-0 space-y-2">
            <Skeleton className="h-4 w-24" />
            <Skeleton className="h-20 w-full" />
            <Skeleton className="h-20 w-full" />
          </div>
        ))}
      </div>
    );
  }

  if (error) {
    const msg = error instanceof ApiError ? error.message : "Unexpected error";
    return <EmptyState icon={ServerCrash} title="Can't load tasks" description={msg} />;
  }

  if (!tasks || tasks.length === 0) {
    return (
      <EmptyState
        icon={Inbox}
        title="No tasks yet"
        description="Author a task (＋ New task), then drag it into Ready to promote it — the loop claims ready tasks and runs them."
      />
    );
  }

  const groups = groupByStatus(tasks);
  const dragging = draggingId ? byId.get(draggingId) : undefined;

  // The dropped id comes from the drag payload (race-free), falling back to the
  // tracked drag state; the task is looked up fresh so a mid-drag refetch can't
  // strand the drop.
  function handleDrop(to: Status, droppedId: string) {
    const id = droppedId || draggingId;
    setDraggingId(null);
    const task = id ? byId.get(id) : undefined;
    if (!task) return;
    const action = dropAction(task, to, byId);
    if (action === "ready") ready.mutate(task.id);
    else if (action === "block") setBlockingId(task.id);
  }

  return (
    <>
      <div className="flex h-full gap-4 overflow-x-auto pb-2">
        {BOARD_COLUMNS.map((status) => (
          <BoardColumn
            key={status}
            status={status}
            tasks={groups[status]}
            selectedId={selectedId}
            draggingId={draggingId}
            draggingFrom={dragging?.status ?? null}
            onSelect={onSelect}
            onDragStart={setDraggingId}
            onDragEnd={() => setDraggingId(null)}
            onDrop={handleDrop}
            blockedReason={(t) => promoteBlockedReason(t, byId)}
          />
        ))}
      </div>

      <BlockReasonDialog
        taskId={blockingId}
        onClose={() => setBlockingId(null)}
      />
    </>
  );
}
