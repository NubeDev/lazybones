import { useState } from "react";
import { Play, Ban, Info } from "lucide-react";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogTrigger,
  DialogClose,
} from "@/components/ui/dialog";
import { ApiError } from "@/lib/api/client";
import { useStartWorkflow, useCancelWorkflow } from "@/lib/hooks/use-workflows";
import type { WorkflowDetail } from "@/types/workflow";

/** Start + Cancel for a workflow. Start only promotes eligible roots to ready
 *  (never claims). Both are disabled when the derived state makes them no-ops. */
export function WorkflowControls({ workflow }: { workflow: WorkflowDetail }) {
  const start = useStartWorkflow();
  const cancel = useCancelWorkflow();
  const [promoted, setPromoted] = useState<string[] | null>(null);

  const terminal = workflow.state === "done" || workflow.state === "cancelled";
  // Starting is only meaningful from draft/ready; running has nothing new to root-promote.
  const startDisabled = start.isPending || terminal || workflow.state === "running";

  const startErr = start.error instanceof ApiError ? start.error.message : null;

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
    </div>
  );
}
