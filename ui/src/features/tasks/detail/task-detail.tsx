import { X, GitCommitHorizontal, Pencil } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Separator } from "@/components/ui/separator";
import { StatusBadge } from "@/components/ui/status-badge";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Skeleton } from "@/components/ui/skeleton";
import { WORKTREE_MODES } from "@/features/tasks/worktree-mode";
import { FieldRow, Mono } from "./field-row";
import { DepsList } from "./deps-list";
import { BlockDialog } from "./block-dialog";
import { DeleteDialog } from "./delete-dialog";
import { SpecView } from "./spec-view";
import { TaskDialog } from "../authoring/task-dialog";
import { StartDialog } from "../start-dialog";
import { useTask } from "@/lib/hooks/use-tasks";
import { relativeTime } from "@/lib/utils/platform";
import type { Task } from "@/types/task";

/** The right-hand inspector for the selected task. */
export function TaskDetail({
  id,
  byId,
  onClose,
  onSelect,
}: {
  id: string;
  byId: Map<string, Task>;
  onClose: () => void;
  onSelect: (id: string) => void;
}) {
  // Prefer fresh detail, fall back to the list snapshot for instant paint.
  const { data, isLoading } = useTask(id);
  const task = data ?? byId.get(id);

  if (!task) {
    return (
      <Panel onClose={onClose}>
        <div className="space-y-3 p-5">
          <Skeleton className="h-6 w-32" />
          <Skeleton className="h-4 w-full" />
          <Skeleton className="h-40 w-full" />
        </div>
      </Panel>
    );
  }

  const terminal = task.status === "done" || task.status === "blocked";

  return (
    <Panel onClose={onClose}>
      <div className="flex items-start justify-between gap-3 border-b border-border p-5">
        <div className="min-w-0">
          <div className="flex items-center gap-2">
            <span className="font-mono text-sm font-bold">{task.id}</span>
            <StatusBadge status={task.status} />
          </div>
          <p className="mt-1 text-sm text-muted-foreground">{task.title}</p>
        </div>
        <Button variant="ghost" size="icon-sm" onClick={onClose} aria-label="Close">
          <X />
        </Button>
      </div>

      <ScrollArea className="flex-1">
        <div className="space-y-5 p-5">
          <section>
            <SectionLabel>Spec</SectionLabel>
            <SpecView spec={task.spec} loading={isLoading && !data} />
          </section>

          <Separator />

          <section>
            <SectionLabel>Dependencies</SectionLabel>
            <DepsList deps={task.deps} byId={byId} onSelect={onSelect} />
          </section>

          <Separator />

          <section>
            <SectionLabel>Details</SectionLabel>
            <div className="divide-y divide-border">
              <FieldRow label="Run">{task.run}</FieldRow>
              <FieldRow label="Tool">{task.tool ?? "config default"}</FieldRow>
              {task.session && <FieldRow label="Session"><Mono>{task.session}</Mono></FieldRow>}
              <FieldRow label="Worktree mode">
                {WORKTREE_MODES[task.worktree_mode_override ?? task.worktree_mode].label}
                {task.worktree_mode_override && (
                  <span className="ml-1 text-[10px] text-muted-foreground">(override)</span>
                )}
              </FieldRow>
              {task.reuse_from && (
                <FieldRow label="Reuses worktree of"><Mono>{task.reuse_from}</Mono></FieldRow>
              )}
              {task.worktree && (
                <FieldRow label="Worktree"><Mono>{task.worktree}</Mono></FieldRow>
              )}
              {task.branch && <FieldRow label="Branch"><Mono>{task.branch}</Mono></FieldRow>}
              {task.commit && (
                <FieldRow label="Commit">
                  <span className="inline-flex items-center gap-1">
                    <GitCommitHorizontal className="size-3 text-muted-foreground" />
                    <Mono>{task.commit.slice(0, 12)}</Mono>
                  </span>
                </FieldRow>
              )}
              {task.heartbeat && (
                <FieldRow label="Heartbeat">{relativeTime(task.heartbeat)}</FieldRow>
              )}
              {task.owns.length > 0 && (
                <FieldRow label="Owns">
                  <div className="flex flex-col items-end gap-0.5">
                    {task.owns.map((g) => (
                      <Mono key={g}>{g}</Mono>
                    ))}
                  </div>
                </FieldRow>
              )}
            </div>
          </section>

          {task.reason && (
            <div className="rounded-md border border-status-blocked/30 bg-status-blocked/10 p-3">
              <p className="text-[11px] font-medium uppercase tracking-wide text-status-blocked">
                Blocked
              </p>
              <p className="mt-1 text-xs text-foreground">{task.reason}</p>
            </div>
          )}
        </div>
      </ScrollArea>

      <div className="flex items-center justify-between gap-2 border-t border-border p-4">
        <DeleteDialog taskId={task.id} onDeleted={onClose} />
        <div className="flex gap-2">
          {task.status === "pending" && <StartDialog task={task} byId={byId} />}
          <TaskDialog
            task={task}
            allTasks={[...byId.values()]}
            trigger={
              <Button variant="secondary" size="sm">
                <Pencil /> Edit
              </Button>
            }
          />
          {!terminal && <BlockDialog taskId={task.id} />}
        </div>
      </div>
    </Panel>
  );
}

function Panel({ children, onClose }: { children: React.ReactNode; onClose: () => void }) {
  return (
    <>
      {/* Mobile scrim */}
      <div className="fixed inset-0 z-20 bg-black/40 lg:hidden" onClick={onClose} />
      <div className="fixed inset-y-0 right-0 z-30 flex w-full max-w-md flex-col border-l border-border bg-surface shadow-2xl animate-fade-up lg:static lg:z-0 lg:shadow-none">
        {children}
      </div>
    </>
  );
}

function SectionLabel({ children }: { children: React.ReactNode }) {
  return (
    <h4 className="mb-2 text-[11px] font-semibold uppercase tracking-wider text-muted-foreground">
      {children}
    </h4>
  );
}
