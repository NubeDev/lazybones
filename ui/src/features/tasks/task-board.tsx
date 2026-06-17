import { BoardColumn } from "./board-column";
import { BOARD_COLUMNS, groupByStatus } from "./group-tasks";
import { Skeleton } from "@/components/ui/skeleton";
import { EmptyState } from "@/components/ui/empty-state";
import { ServerCrash, Inbox } from "lucide-react";
import type { Task } from "@/types/task";
import { ApiError } from "@/lib/api/client";

/** The kanban board: one column per lifecycle status, horizontally scrolling. */
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
    return (
      <EmptyState
        icon={ServerCrash}
        title="Can't load tasks"
        description={msg}
      />
    );
  }

  if (!tasks || tasks.length === 0) {
    return (
      <EmptyState
        icon={Inbox}
        title="No tasks yet"
        description="Import a workfile.yaml into lazybonesd, then promote ready tasks to populate the board."
      />
    );
  }

  const groups = groupByStatus(tasks);

  return (
    <div className="flex h-full gap-4 overflow-x-auto pb-2">
      {BOARD_COLUMNS.map((status) => (
        <BoardColumn
          key={status}
          status={status}
          tasks={groups[status]}
          selectedId={selectedId}
          onSelect={onSelect}
        />
      ))}
    </div>
  );
}
