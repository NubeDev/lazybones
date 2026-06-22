import { ChevronRight, Clock } from "lucide-react";
import { StatusBadge } from "@/components/ui/status-badge";
import { EmptyState } from "@/components/ui/empty-state";
import { GitBranch } from "lucide-react";
import { layerTasks } from "./plan-graph";
import { duration, shortTime } from "@/lib/utils/platform";
import type { Task } from "@/types/task";
import type { Workspace } from "@/types/workflow";

/** The Plan tab: the workflow's tasks laid out by dependency depth, left→right,
 *  so fan-out is visible (e.g. scaffold → {a,b,c} → integrate → verify). Keyed
 *  off `deps` / `run_id`, never off parsing the dotted label. */
export function PlanGraphView({
  tasks,
  defaults,
  onSelect,
}: {
  tasks: Task[];
  /** Workflow-level agent defaults; a task's `null` field inherits from here. */
  defaults?: Workspace;
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
                defaults={defaults}
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
  defaults,
  onSelect,
}: {
  task: Task;
  defaults?: Workspace;
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
      <TaskAgent task={task} defaults={defaults} />
      <TaskTiming task={task} />
    </button>
  );
}

/** The effective agent the task runs under: tool / model / effort. A `null`
 *  field inherits the workflow default, which we still show (dimmed) so every
 *  card reads as a complete agent spec rather than a sparse one. */
function TaskAgent({ task, defaults }: { task: Task; defaults?: Workspace }) {
  const fields = [
    { own: task.tool, fallback: defaults?.tool },
    { own: task.model, fallback: defaults?.model },
    { own: task.effort, fallback: defaults?.effort },
  ];
  const parts = fields
    .map((f) => ({
      value: f.own ?? f.fallback,
      inherited: !f.own && Boolean(f.fallback),
    }))
    .filter((f) => Boolean(f.value));
  if (parts.length === 0) return null;
  return (
    <span className="flex flex-wrap items-center gap-1">
      {parts.map((p) => (
        <span
          key={p.value}
          className={`truncate font-mono text-[10px] ${
            p.inherited ? "text-muted-foreground/40" : "text-muted-foreground/70"
          }`}
        >
          {p.value}
        </span>
      ))}
    </span>
  );
}

/** A compact timing line per task card: live elapsed while running, total
 *  duration + finish time once done, and nothing before it has started. */
function TaskTiming({ task }: { task: Task }) {
  if (!task.started_at) return null;
  const running = task.status === "running" || task.status === "gating";
  return (
    <span className="flex items-center gap-1 text-[10px] text-muted-foreground/70">
      <Clock className="size-2.5" />
      <span>{duration(task.started_at, task.finished_at)}</span>
      {running ? (
        <span className="text-muted-foreground/50">· running</span>
      ) : (
        task.finished_at && (
          <span className="text-muted-foreground/50">
            · {shortTime(task.finished_at)}
          </span>
        )
      )}
    </span>
  );
}
