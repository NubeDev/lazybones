import { GitBranch, Boxes, Clock } from "lucide-react";
import { Card } from "@/components/ui/card";
import { StatusDot } from "@/components/ui/status-badge";
import { cn } from "@/lib/utils/cn";
import { relativeTime } from "@/lib/utils/platform";
import type { Task } from "@/types/task";

/** A compact, clickable task tile for the board columns. */
export function TaskCard({
  task,
  selected,
  onSelect,
}: {
  task: Task;
  selected?: boolean;
  onSelect: (id: string) => void;
}) {
  return (
    <Card
      onClick={() => onSelect(task.id)}
      className={cn(
        "cursor-pointer p-3 transition-all hover:border-border-strong hover:bg-surface-2",
        "animate-fade-up",
        selected && "border-accent/50 ring-1 ring-accent/30",
      )}
    >
      <div className="flex items-start justify-between gap-2">
        <div className="flex min-w-0 items-center gap-2">
          <StatusDot status={task.status} />
          <span className="truncate font-mono text-xs font-semibold">{task.id}</span>
        </div>
        {task.tool && (
          <span className="shrink-0 rounded bg-muted px-1.5 py-0.5 text-[10px] font-medium text-muted-foreground">
            {task.tool}
          </span>
        )}
      </div>

      <p className="mt-2 line-clamp-2 text-xs leading-snug text-muted-foreground">
        {task.title}
      </p>

      <div className="mt-3 flex flex-wrap items-center gap-x-3 gap-y-1 text-[10px] text-muted-foreground/80">
        {task.deps.length > 0 && (
          <span className="inline-flex items-center gap-1">
            <Boxes className="size-3" />
            {task.deps.length} dep{task.deps.length > 1 ? "s" : ""}
          </span>
        )}
        {task.branch && (
          <span className="inline-flex items-center gap-1 truncate">
            <GitBranch className="size-3" />
            <span className="truncate">{task.branch}</span>
          </span>
        )}
        {task.heartbeat && task.status === "running" && (
          <span className="inline-flex items-center gap-1">
            <Clock className="size-3" />
            {relativeTime(task.heartbeat)}
          </span>
        )}
      </div>
    </Card>
  );
}
