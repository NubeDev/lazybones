import { useState } from "react";
import { Play, Ban, Info, Trash2, RotateCcw, PlayCircle } from "lucide-react";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogTrigger,
  DialogClose,
} from "@/components/ui/dialog";
import { ApiError } from "@/lib/api/client";
import {
  useStartWorkflow,
  useCancelWorkflow,
  useRestartWorkflow,
  useResumeWorkflow,
  useDeleteWorkflow,
} from "@/lib/hooks/use-workflows";
import type { WorkflowDetail } from "@/types/workflow";

/** Start / Cancel / Delete for a workflow. Start only promotes eligible roots to
 *  ready (never claims). Controls are disabled when the derived state makes them
 *  no-ops. `onDeleted` (when given) fires after a successful delete so the detail
 *  view can navigate back to the list. */
export function WorkflowControls({
  workflow,
  onDeleted,
}: {
  workflow: WorkflowDetail;
  onDeleted?: () => void;
}) {
  const start = useStartWorkflow();
  const cancel = useCancelWorkflow();
  const restart = useRestartWorkflow();
  const resume = useResumeWorkflow();
  const del = useDeleteWorkflow();
  const [promoted, setPromoted] = useState<string[] | null>(null);
  const [includeDone, setIncludeDone] = useState(false);
  const [removeWorktrees, setRemoveWorktrees] = useState(false);

  const terminal = workflow.state === "done" || workflow.state === "cancelled";
  // Starting is only meaningful from draft/ready; running has nothing new to root-promote.
  const startDisabled = start.isPending || terminal || workflow.state === "running";
  // Delete refuses (409) while tasks are live — cancel first. Mirror that here.
  const deleteBlocked = workflow.state === "running";
  // A fresh draft (no task has moved off pending) has nothing to reset.
  const restartDisabled = restart.isPending || workflow.state === "draft";
  // Resume is only meaningful when a task is stuck — i.e. needs attention.
  const needsAttention = workflow.state === "needs-attention";

  const startErr = start.error instanceof ApiError ? start.error.message : null;
  const restartErr = restart.error instanceof ApiError ? restart.error.message : null;
  const resumeErr = resume.error instanceof ApiError ? resume.error.message : null;
  const delErr = del.error instanceof ApiError ? del.error.message : null;

  return (
    <div className="flex items-center gap-2">
      {promoted && (
        <span className="text-[11px] text-muted-foreground">
          {promoted.length === 0
            ? "Nothing eligible to promote yet"
            : `Promoted: ${promoted.join(", ")}`}
        </span>
      )}
      {startErr && <span className="text-[11px] text-status-blocked">{startErr}</span>}
      {restartErr && <span className="text-[11px] text-status-blocked">{restartErr}</span>}
      {resumeErr && <span className="text-[11px] text-status-blocked">{resumeErr}</span>}
      {delErr && <span className="text-[11px] text-status-blocked">{delErr}</span>}

      <Button
        size="sm"
        disabled={startDisabled}
        title={
          terminal
            ? `Workflow is ${workflow.state}`
            : workflow.state === "running"
              ? "Already running — eligible roots are promoted"
              : "Promote eligible root tasks to ready"
        }
        onClick={() =>
          start.mutate(workflow.id, {
            onSuccess: (res) => setPromoted(res.promoted),
          })
        }
      >
        <Play /> Start
      </Button>

      <Dialog>
        <DialogTrigger asChild>
          <Button
            variant="destructive"
            size="sm"
            disabled={cancel.isPending || terminal}
            title={terminal ? `Workflow is ${workflow.state}` : "Cancel this workflow"}
          >
            <Ban /> Cancel
          </Button>
        </DialogTrigger>
        <DialogContent
          title={`Cancel ${workflow.id}?`}
          description="Blocks unclaimed tasks and kills any running agents for this workflow. This can't be undone."
        >
          <p className="mt-2 flex items-start gap-1.5 text-[11px] text-muted-foreground">
            <Info className="mt-0.5 size-3 shrink-0" />
            Tasks already <b className="mx-1 font-medium">done</b> stay done; in-flight
            agents are stopped.
          </p>
          <div className="mt-4 flex justify-end gap-2">
            <DialogClose asChild>
              <Button variant="ghost" size="sm">
                Keep running
              </Button>
            </DialogClose>
            <DialogClose asChild>
              <Button
                variant="destructive"
                size="sm"
                disabled={cancel.isPending}
                onClick={() => cancel.mutate(workflow.id)}
              >
                <Ban /> Cancel workflow
              </Button>
            </DialogClose>
          </div>
        </DialogContent>
      </Dialog>

      {needsAttention && (
        <Button
          variant="outline"
          size="sm"
          disabled={resume.isPending}
          title="Reset only the blocked tasks to pending and continue from where it broke"
          onClick={() => resume.mutate(workflow.id)}
        >
          <PlayCircle /> Resume
        </Button>
      )}

      <Dialog>
        <DialogTrigger asChild>
          <Button
            variant="outline"
            size="sm"
            disabled={restartDisabled}
            title={
              workflow.state === "draft"
                ? "Nothing to restart yet"
                : "Reset this workflow's tasks to run from the beginning"
            }
          >
            <RotateCcw /> Restart
          </Button>
        </DialogTrigger>
        <DialogContent
          title={`Restart ${workflow.id}?`}
          description="Resets this workflow's tasks back to pending so it can run from the beginning. Any running agents are stopped. It does not auto-start — press Start when ready."
        >
          <div className="mt-3 space-y-2">
            <label className="flex cursor-pointer items-start gap-2 text-xs">
              <input
                type="checkbox"
                className="mt-0.5"
                checked={includeDone}
                onChange={(e) => setIncludeDone(e.target.checked)}
              />
              <span>
                <b className="font-medium">Also reset done tasks</b>
                <span className="block text-muted-foreground">
                  Re-run everything from scratch. Off: keep completed tasks, redo only
                  the unfinished ones.
                </span>
              </span>
            </label>
            <label className="flex cursor-pointer items-start gap-2 text-xs">
              <input
                type="checkbox"
                className="mt-0.5"
                checked={removeWorktrees}
                onChange={(e) => setRemoveWorktrees(e.target.checked)}
              />
              <span>
                <b className="font-medium">Remove worktrees</b>
                <span className="block text-muted-foreground">
                  Tear down each task's git worktree for a truly clean start. Off:
                  leave them to be reused.
                </span>
              </span>
            </label>
          </div>
          <div className="mt-4 flex justify-end gap-2">
            <DialogClose asChild>
              <Button variant="ghost" size="sm">
                Cancel
              </Button>
            </DialogClose>
            <DialogClose asChild>
              <Button
                size="sm"
                disabled={restart.isPending}
                onClick={() =>
                  restart.mutate({
                    id: workflow.id,
                    opts: {
                      include_done: includeDone,
                      remove_worktrees: removeWorktrees,
                    },
                  })
                }
              >
                <RotateCcw /> Restart workflow
              </Button>
            </DialogClose>
          </div>
        </DialogContent>
      </Dialog>

      <Dialog>
        <DialogTrigger asChild>
          <Button
            variant="destructive"
            size="sm"
            disabled={del.isPending || deleteBlocked}
            title={
              deleteBlocked
                ? "Cancel the workflow before deleting (it has running tasks)"
                : "Delete this workflow and its tasks"
            }
          >
            <Trash2 /> Delete
          </Button>
        </DialogTrigger>
        <DialogContent
          title={`Delete ${workflow.id}?`}
          description="Permanently removes this workflow and all of its tasks. This can't be undone."
        >
          <p className="mt-2 flex items-start gap-1.5 text-[11px] text-muted-foreground">
            <Info className="mt-0.5 size-3 shrink-0" />
            Unlike <b className="mx-1 font-medium">Cancel</b>, this leaves no record —
            the workflow and its tasks are gone for good.
          </p>
          <div className="mt-4 flex justify-end gap-2">
            <DialogClose asChild>
              <Button variant="ghost" size="sm">
                Keep workflow
              </Button>
            </DialogClose>
            <DialogClose asChild>
              <Button
                variant="destructive"
                size="sm"
                disabled={del.isPending}
                onClick={() =>
                  del.mutate(workflow.id, { onSuccess: () => onDeleted?.() })
                }
              >
                <Trash2 /> Delete workflow
              </Button>
            </DialogClose>
          </div>
        </DialogContent>
      </Dialog>
    </div>
  );
}
