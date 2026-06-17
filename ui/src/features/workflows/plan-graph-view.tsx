import { ChevronRight } from "lucide-react";
import { StatusBadge } from "@/components/ui/status-badge";
import { EmptyState } from "@/components/ui/empty-state";
import { GitBranch } from "lucide-react";
import { layerTasks } from "./plan-graph";
import type { Task } from "@/types/task";

/** The Plan tab: the workflow's tasks laid out by dependency depth, left→right,
 *  so fan-out is visible (e.g. scaffold → {a,b,c} → integrate → verify). Keyed
 *  off `deps` / `run_id`, never off parsing the dotted label. */
export function PlanGraphView({
  tasks,
  onSelect,
}: {
  tasks: Task[];
  onSelect?: (id: string) => void;
}) {
  if (tasks.length === 0) {
    return (
      <EmptyState
        icon={GitBranch}
        title="No tasks yet"
        description="Add tasks to this workflow — their dependency graph renders here."
      />
    );
  }

  const layers = layerTasks(tasks);

  return (
    <div className="flex items-stretch gap-2 overflow-x-auto pb-2">
      {layers.map((layer, i) => (
        <div key={i} className="flex items-stretch gap-2">
          <div className="flex min-w-[12rem] flex-col gap-2">
            {layer.map((node) => (
              <GraphNodeCard
                key={node.task.id}
                task={node.task}
                onSelect={onSelect}
              />
            ))}
          </div>
          {i < layers.length - 1 && (
            <div className="flex items-center text-muted-foreground/50">
              <ChevronRight className="size-5" />
            </div>
          )}
        </div>
      ))}
    </div>
  );
}

function GraphNodeCard({
  task,
  onSelect,
}: {
  task: Task;
  onSelect?: (id: string) => void;
}) {
  return (
    <button
      type="button"
      onClick={() => onSelect?.(task.id)}
      className="flex flex-col gap-1.5 rounded-md border border-border bg-surface p-3 text-left transition-colors hover:border-border-strong"
    >
      <div className="flex items-center justify-between gap-2">
        <span className="truncate text-xs font-medium">{task.title}</span>
        <StatusBadge status={task.status} iconOnly />
      </div>
      <span className="font-mono text-[10px] text-muted-foreground">{task.id}</span>
      {task.deps.length > 0 && (
        <span className="truncate font-mono text-[10px] text-muted-foreground/70">
          ← {task.deps.join(", ")}
        </span>
      )}
    </button>
  );
}
