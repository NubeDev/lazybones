import { useState } from "react";
import {
  Play,
  PlayCircle,
  Octagon,
  PauseCircle,
  Eraser,
  Info,
  Trash2,
  RotateCcw,
  MoreHorizontal,
  Settings2,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogClose } from "@/components/ui/dialog";
import {
  DropdownMenu,
  DropdownMenuTrigger,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
} from "@/components/ui/dropdown-menu";
import { EditWorkflowDialog } from "./edit-workflow-dialog";
import { ApiError } from "@/lib/api/client";
import {
  useStartWorkflow,
  useStopWorkflow,
  useStopResetWorkflow,
  useRestartWorkflow,
  useResumeWorkflow,
  useDeleteWorkflow,
} from "@/lib/hooks/use-workflows";
import type { WorkflowDetail } from "@/types/workflow";

type ActiveDialog = "stop" | "restart" | "delete" | "edit" | null;

/** Workflow header controls — collapsed to one contextual primary button plus a
 *  `⋯` overflow menu, so only the actions that make sense for the current derived
 *  state are ever shown:
 *
 *  | state            | primary | menu             |
 *  | draft            | Start   | Delete           |
 *  | ready / running  | Stop    | Restart          |
 *  | needs-attention  | Resume  | Restart, Delete  |
 *  | stopped          | Resume  | Restart, Delete  |
 *  | done             | —       | Restart, Delete  |
 *
 *  Destructive / multi-option actions (Stop, Restart, Delete) open confirm
 *  dialogs. Only `done` is terminal; a stopped run is paused, not finished.
 *  `onDeleted` (when given) fires after a successful delete so the detail view can
 *  navigate back to the list. */
export function WorkflowControls({
  workflow,
  onDeleted,
}: {
  workflow: WorkflowDetail;
  onDeleted?: () => void;
}) {
  const start = useStartWorkflow();
  const stop = useStopWorkflow();
  const stopReset = useStopResetWorkflow();
  const restart = useRestartWorkflow();
  const resume = useResumeWorkflow();
  const del = useDeleteWorkflow();

  const [dialog, setDialog] = useState<ActiveDialog>(null);
  const [promoted, setPromoted] = useState<string[] | null>(null);
  const [includeDone, setIncludeDone] = useState(false);
  const [removeWorktrees, setRemoveWorktrees] = useState(false);

  const state = workflow.state;
  const isDraft = state === "draft";
  const isLive = state === "running" || state === "ready"; // active, work in flight
  const isStopped = state === "stopped";
  const needsAttention = state === "needs-attention";
  // `done` falls through to no primary button + the ⋯ menu (Restart / Delete).

  // Which secondary actions belong in the overflow menu for this state.
  const canRestart = !isDraft; // a fresh draft has nothing to reset
  const canDelete = !isLive; // delete refuses (409) while tasks are live

  // Surface the first action error (if any) as a single inline line.
  const err = [start, stop, stopReset, restart, resume, del]
    .map((m) => (m.error instanceof ApiError ? m.error.message : null))
    .find(Boolean);

  const busy =
    start.isPending ||
    stop.isPending ||
    stopReset.isPending ||
    restart.isPending ||
    resume.isPending ||
    del.isPending;

  return (
    <div className="flex items-center gap-2">
      {promoted && (
        <span className="text-[11px] text-muted-foreground">
          {promoted.length === 0
            ? "Nothing eligible to promote yet"
            : `Promoted: ${promoted.join(", ")}`}
        </span>
      )}
      {err && <span className="text-[11px] text-status-blocked">{err}</span>}

      {/* Primary contextual action. */}
      {isDraft && (
        <Button
          size="sm"
          disabled={start.isPending}
          title="Promote eligible root tasks to ready"
          onClick={() =>
            start.mutate(workflow.id, { onSuccess: (res) => setPromoted(res.promoted) })
          }
        >
          <Play /> Start
        </Button>
      )}

      {isLive && (
        <Button
          size="sm"
          variant="outline"
          disabled={stop.isPending || stopReset.isPending}
          title="Pause this workflow"
          onClick={() => setDialog("stop")}
        >
          <Octagon /> Stop
        </Button>
      )}

      {(isStopped || needsAttention) && (
        <Button
          size="sm"
          disabled={resume.isPending}
          title={
            isStopped
              ? "Resume — the scheduler picks up where it left off"
              : "Reset the blocked tasks to pending and continue from where it broke"
          }
          onClick={() => resume.mutate(workflow.id)}
        >
          <PlayCircle /> Resume
        </Button>
      )}

      {/* Overflow menu — Edit is always available; Restart/Delete only when valid. */}
      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <Button
            size="icon-sm"
            variant="ghost"
            disabled={busy}
            title="More actions"
            aria-label="More actions"
          >
            <MoreHorizontal />
          </Button>
        </DropdownMenuTrigger>
        <DropdownMenuContent>
          <DropdownMenuItem onSelect={() => setDialog("edit")}>
            <Settings2 /> Edit workflow…
          </DropdownMenuItem>
          {(canRestart || canDelete) && <DropdownMenuSeparator />}
          {canRestart && (
            <DropdownMenuItem onSelect={() => setDialog("restart")}>
              <RotateCcw /> Restart…
            </DropdownMenuItem>
          )}
          {canRestart && canDelete && <DropdownMenuSeparator />}
          {canDelete && (
            <DropdownMenuItem tone="danger" onSelect={() => setDialog("delete")}>
              <Trash2 /> Delete…
            </DropdownMenuItem>
          )}
        </DropdownMenuContent>
      </DropdownMenu>

      {/* Edit workflow dialog — controlled from the menu item above. */}
      <EditWorkflowDialog
        workflow={workflow}
        open={dialog === "edit"}
        onOpenChange={(o) => setDialog(o ? "edit" : null)}
      />

      {/* ---- Stop dialog: pause (keep work) vs pause + reset ---- */}
      <Dialog open={dialog === "stop"} onOpenChange={(o) => !o && setDialog(null)}>
        <DialogContent
          title={`Stop ${workflow.id}?`}
          description="Pause this workflow. The scheduler promotes and claims nothing until you resume — both options below are reversible."
        >
          <p className="flex items-start gap-1.5 text-[11px] text-muted-foreground">
            <Info className="mt-0.5 size-3 shrink-0" />
            Tasks already <b className="mx-1 font-medium">done</b> stay done; in-flight
            agents are stopped either way. Not terminal — use{" "}
            <b className="mx-1 font-medium">Delete</b> to archive.
          </p>
          <div className="mt-4 grid gap-2">
            <Button
              variant="outline"
              size="sm"
              className="h-auto w-full min-w-0 flex-col items-start gap-0.5 py-2 text-left"
              disabled={stop.isPending}
              onClick={() => {
                stop.mutate(workflow.id);
                setDialog(null);
              }}
            >
              <span className="flex items-center gap-1.5 font-medium">
                <PauseCircle className="size-3.5" /> Stop (keep work)
              </span>
              <span className="whitespace-normal text-[11px] font-normal text-muted-foreground">
                Pause and reclaim running tasks back to ready — no work is lost.
              </span>
            </Button>
            <Button
              variant="outline"
              size="sm"
              className="h-auto w-full min-w-0 flex-col items-start gap-0.5 py-2 text-left"
              disabled={stopReset.isPending}
              onClick={() => {
                stopReset.mutate(workflow.id);
                setDialog(null);
              }}
            >
              <span className="flex items-center gap-1.5 font-medium">
                <Eraser className="size-3.5" /> Stop &amp; reset
              </span>
              <span className="whitespace-normal text-[11px] font-normal text-muted-foreground">
                Pause and reset unfinished tasks to pending — throw in-flight progress
                away. Done tasks are kept; still resumable.
              </span>
            </Button>
          </div>
          <div className="mt-3 flex justify-end">
            <DialogClose asChild>
              <Button variant="ghost" size="sm">
                Keep running
              </Button>
            </DialogClose>
          </div>
        </DialogContent>
      </Dialog>

      {/* ---- Restart dialog ---- */}
      <Dialog open={dialog === "restart"} onOpenChange={(o) => !o && setDialog(null)}>
        <DialogContent
          title={`Restart ${workflow.id}?`}
          description="Resets this workflow's tasks back to pending so it can run from the beginning. Any running agents are stopped. It does not auto-start — press Start when ready."
        >
          <div className="space-y-2">
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
            <Button
              size="sm"
              disabled={restart.isPending}
              onClick={() => {
                restart.mutate({
                  id: workflow.id,
                  opts: { include_done: includeDone, remove_worktrees: removeWorktrees },
                });
                setDialog(null);
              }}
            >
              <RotateCcw /> Restart workflow
            </Button>
          </div>
        </DialogContent>
      </Dialog>

      {/* ---- Delete dialog ---- */}
      <Dialog open={dialog === "delete"} onOpenChange={(o) => !o && setDialog(null)}>
        <DialogContent
          title={`Delete ${workflow.id}?`}
          description="Permanently removes this workflow and all of its tasks. This can't be undone."
        >
          <p className="flex items-start gap-1.5 text-[11px] text-muted-foreground">
            <Info className="mt-0.5 size-3 shrink-0" />
            Unlike <b className="mx-1 font-medium">Stop</b>, this leaves no record — the
            workflow and its tasks are gone for good.
          </p>
          <div className="mt-4 flex justify-end gap-2">
            <DialogClose asChild>
              <Button variant="ghost" size="sm">
                Keep workflow
              </Button>
            </DialogClose>
            <Button
              variant="destructive"
              size="sm"
              disabled={del.isPending}
              onClick={() =>
                del.mutate(workflow.id, {
                  onSuccess: () => {
                    setDialog(null);
                    onDeleted?.();
                  },
                })
              }
            >
              <Trash2 /> Delete workflow
            </Button>
          </div>
        </DialogContent>
      </Dialog>
    </div>
  );
}
