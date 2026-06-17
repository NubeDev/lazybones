import { useState } from "react";
import { Ban } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Dialog, DialogContent, DialogTrigger, DialogClose } from "@/components/ui/dialog";
import { useBlock } from "@/lib/hooks/use-tasks";

/** Confirm + reason capture for forcing a task to `blocked`. */
export function BlockDialog({ taskId }: { taskId: string }) {
  const [reason, setReason] = useState("");
  const block = useBlock();

  return (
    <Dialog>
      <DialogTrigger asChild>
        <Button variant="destructive" size="sm">
          <Ban /> Block
        </Button>
      </DialogTrigger>
      <DialogContent
        title={`Block ${taskId}`}
        description="Force this task to blocked. Its worktree is kept for triage."
      >
        <Input
          autoFocus
          placeholder="Reason (recorded on the task + run log)"
          value={reason}
          onChange={(e) => setReason(e.target.value)}
        />
        <div className="mt-4 flex justify-end gap-2">
          <DialogClose asChild>
            <Button variant="ghost" size="sm">
              Cancel
            </Button>
          </DialogClose>
          <DialogClose asChild>
            <Button
              variant="destructive"
              size="sm"
              disabled={!reason.trim() || block.isPending}
              onClick={() => block.mutate({ id: taskId, reason: reason.trim() })}
            >
              Block task
            </Button>
          </DialogClose>
        </div>
      </DialogContent>
    </Dialog>
  );
}
