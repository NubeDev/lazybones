import { useState } from "react";
import { Play, Info } from "lucide-react";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogTrigger,
  DialogClose,
} from "@/components/ui/dialog";
import { useReadyTask, useUpdateTask } from "@/lib/hooks/use-tasks";
import { promoteBlockedReason } from "./drag-rules";
import { WorktreeModePicker } from "./worktree-mode";
import type { Task, WorktreeMode } from "@/types/task";

/** Start a task: pick its worktree mode, then promote it to `ready` so the run
 *  loop can claim and execute it. This is the human's only "go" signal — the UI
 *  never marks a task `running` itself (that would create a task no agent is
 *  actually running); the loop does the claim once it sees the task ready.
 *
 *  Only offered for `pending` tasks whose dependencies are all `done` (otherwise
 *  promoting is a no-op the backend rejects). `byId` resolves that dep check. */
export function StartDialog({
  task,
  byId,
  trigger,
}: {
  task: Task;
  byId: Map<string, Task>;
  trigger?: React.ReactNode;
}) {
  const [mode, setMode] = useState<WorktreeMode>(task.worktree_mode);
  const ready = useReadyTask();
  const update = useUpdateTask();

  const blocked = promoteBlockedReason(task, byId);
  const busy = ready.isPending || update.isPending;

  async function start() {
    // Persist the chosen worktree mode only if it changed, then promote. The
    // PATCH carries the task's existing authored fields unchanged.
    if (mode !== task.worktree_mode) {
      await update.mutateAsync({
        id: task.id,
        draft: {
          title: task.title,
          spec: task.spec,
          deps: task.deps,
          owns: task.owns,
          tool: task.tool,
          worktree_mode: mode,
        },
      });
    }
    await ready.mutateAsync(task.id);
  }

  return (
    <Dialog>
      <DialogTrigger asChild>
        {trigger ?? (
          <Button variant="default" size="sm">
            <Play /> Start
          </Button>
        )}
      </DialogTrigger>
      <DialogContent
        title={`Start ${task.id}`}
        description="Promote this task to ready so the run loop can claim and run it."
      >
        {blocked ? (
          <p className="rounded-md border border-amber-500/30 bg-amber-500/10 px-3 py-2 text-xs text-amber-600 dark:text-amber-400">
            {blocked} — finish those first, then start this one.
          </p>
        ) : (
          <>
            <div className="space-y-1.5">
              <span className="text-xs font-medium">Worktree</span>
              <WorktreeModePicker value={mode} onChange={setMode} />
            </div>

            <p className="mt-3 flex items-start gap-1.5 text-[11px] text-muted-foreground">
              <Info className="mt-0.5 size-3 shrink-0" />
              Starting only promotes the task to <b className="font-medium">ready</b>.
              The <code>lazybones-run</code> loop picks it up and runs it — if no
              loop is running, it waits in Ready until one is.
            </p>

            <div className="mt-4 flex justify-end gap-2">
              <DialogClose asChild>
                <Button variant="ghost" size="sm">
                  Cancel
                </Button>
              </DialogClose>
              <DialogClose asChild>
                <Button variant="default" size="sm" disabled={busy} onClick={start}>
                  <Play /> Promote to ready
                </Button>
              </DialogClose>
            </div>
          </>
        )}
      </DialogContent>
    </Dialog>
  );
}
