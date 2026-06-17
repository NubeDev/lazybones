import { TaskCard } from "./task-card";
import { STATUS_META } from "@/types/status-meta";
import type { Status, Task } from "@/types/task";

/** One lifecycle column on the board. */
export function BoardColumn({
  status,
  tasks,
  selectedId,
  onSelect,
}: {
  status: Status;
  tasks: Task[];
  selectedId: string | null;
  onSelect: (id: string) => void;
}) {
  const meta = STATUS_META[status];
  const Icon = meta.icon;

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

      <div className="flex flex-1 flex-col gap-2 rounded-lg border border-dashed border-border/60 bg-surface-2/40 p-2">
        {tasks.length === 0 ? (
          <p className="px-2 py-6 text-center text-[11px] text-muted-foreground/60">
            empty
          </p>
        ) : (
          tasks.map((t) => (
            <TaskCard
              key={t.id}
              task={t}
              selected={selectedId === t.id}
              onSelect={onSelect}
            />
          ))
        )}
      </div>
    </div>
  );
}
