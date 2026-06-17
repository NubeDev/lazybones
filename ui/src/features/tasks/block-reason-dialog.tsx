import { useState } from "react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Dialog, DialogContent } from "@/components/ui/dialog";
import { useBlock } from "@/lib/hooks/use-tasks";

/** A controlled reason prompt shown when a card is dragged onto the Blocked
 *  column. `taskId` non-null opens it; blocking requires a recorded reason
 *  (SCOPE.md: a red/blocked task keeps its worktree for triage). */
export function BlockReasonDialog({
  taskId,
  onClose,
}: {
  taskId: string | null;
  onClose: () => void;
}) {
  const [reason, setReason] = useState("");
  const block = useBlock();

  function submit() {
    if (!taskId || !reason.trim()) return;
    block.mutate(
      { id: taskId, reason: reason.trim() },
      {
        onSuccess: () => {
          setReason("");
          onClose();
        },
      },
    );
  }

  return (
    <Dialog
      open={taskId !== null}
      onOpenChange={(o) => {
        if (!o) {
          setReason("");
          onClose();
        }
      }}
    >
      {taskId !== null && (
        <DialogContent
          title={`Block ${taskId}`}
          description="Force this task to blocked. Its worktree is kept for triage."
        >
          <Input
            autoFocus
            placeholder="Reason (recorded on the task + run log)"
            value={reason}
            onChange={(e) => setReason(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && submit()}
          />
          <div className="mt-4 flex justify-end gap-2">
            <Button variant="ghost" size="sm" onClick={onClose}>
              Cancel
            </Button>
            <Button
              variant="destructive"
              size="sm"
              disabled={!reason.trim() || block.isPending}
              onClick={submit}
            >
              Block task
            </Button>
          </div>
        </DialogContent>
      )}
    </Dialog>
  );
}
