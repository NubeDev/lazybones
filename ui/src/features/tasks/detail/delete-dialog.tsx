import { Trash2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogTrigger,
  DialogClose,
} from "@/components/ui/dialog";
import { useDeleteTask } from "@/lib/hooks/use-tasks";

/** Confirm + delete a task (and its dependency edges). On success, closes the
 *  inspector via `onDeleted` so it doesn't dangle on a gone task. */
export function DeleteDialog({
  taskId,
  onDeleted,
}: {
  taskId: string;
  onDeleted: () => void;
}) {
  const del = useDeleteTask();

  return (
    <Dialog>
      <DialogTrigger asChild>
        <Button variant="ghost" size="sm">
          <Trash2 className="text-status-blocked" /> Delete
        </Button>
      </DialogTrigger>
      <DialogContent
        title={`Delete ${taskId}`}
        description="Remove this task and its dependency edges. This cannot be undone."
      >
        <div className="flex justify-end gap-2">
          <DialogClose asChild>
            <Button variant="ghost" size="sm">
              Cancel
            </Button>
          </DialogClose>
          <DialogClose asChild>
            <Button
              variant="destructive"
              size="sm"
              disabled={del.isPending}
              onClick={() => del.mutate(taskId, { onSuccess: onDeleted })}
            >
              Delete task
            </Button>
          </DialogClose>
        </div>
      </DialogContent>
    </Dialog>
  );
}
