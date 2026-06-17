import { useState } from "react";
import { Plus } from "lucide-react";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogTrigger,
  DialogClose,
} from "@/components/ui/dialog";
import { ApiError } from "@/lib/api/client";
import { useCreateTask, useUpdateTask } from "@/lib/hooks/use-tasks";
import { TaskFormFields, EMPTY_DRAFT } from "./task-form-fields";
import type { TaskDraft } from "@/lib/api/tasks";
import type { Task } from "@/types/task";

/** Author a task. With `task`, edits it (id locked); without, creates a new one.
 *  `allTasks` supplies the dependency chip-picker (the current task is excluded).
 *  Render your own trigger via `trigger`, or it shows a default "New task" button. */
export function TaskDialog({
  task,
  allTasks,
  trigger,
}: {
  task?: Task;
  allTasks: Task[];
  trigger?: React.ReactNode;
}) {
  const editing = !!task;
  const depCandidates = allTasks
    .filter((t) => t.id !== task?.id)
    .map((t) => t.id);
  const [open, setOpen] = useState(false);
  const [id, setId] = useState(task?.id ?? "");
  const [draft, setDraft] = useState<TaskDraft>(
    task
      ? {
          title: task.title,
          spec: task.spec,
          deps: task.deps,
          owns: task.owns,
          tool: task.tool,
          worktree_mode: task.worktree_mode,
        }
      : EMPTY_DRAFT,
  );

  const create = useCreateTask();
  const update = useUpdateTask();
  const mutation = editing ? update : create;

  function reset() {
    if (!editing) {
      setId("");
      setDraft(EMPTY_DRAFT);
    }
  }

  function submit() {
    const trimmed = id.trim();
    if (!trimmed || !draft.title.trim()) return;
    mutation.mutate(
      { id: trimmed, draft },
      {
        onSuccess: () => {
          setOpen(false);
          reset();
        },
      },
    );
  }

  const err = mutation.error;
  const message =
    err instanceof ApiError
      ? err.status === 409
        ? `A task "${id}" already exists.`
        : err.message
      : err
        ? "Something went wrong."
        : null;

  return (
    <Dialog
      open={open}
      onOpenChange={(o) => {
        setOpen(o);
        if (!o) {
          mutation.reset();
          reset();
        }
      }}
    >
      <DialogTrigger asChild>
        {trigger ?? (
          <Button size="sm">
            <Plus /> New task
          </Button>
        )}
      </DialogTrigger>
      <DialogContent
        title={editing ? `Edit ${task!.id}` : "New task"}
        description={
          editing
            ? "Update the authored fields. Lifecycle + claim state are preserved."
            : "Author a task in the queue. It starts pending until its deps are done."
        }
      >
        <TaskFormFields
          id={id}
          onId={setId}
          lockId={editing}
          draft={draft}
          onDraft={setDraft}
          depCandidates={depCandidates}
        />

        {message && (
          <p className="mt-3 rounded-md border border-status-blocked/30 bg-status-blocked/10 px-3 py-2 text-xs text-status-blocked">
            {message}
          </p>
        )}

        <div className="mt-4 flex justify-end gap-2">
          <DialogClose asChild>
            <Button variant="ghost" size="sm">
              Cancel
            </Button>
          </DialogClose>
          <Button
            size="sm"
            onClick={submit}
            disabled={!id.trim() || !draft.title.trim() || mutation.isPending}
          >
            {editing ? "Save changes" : "Create task"}
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  );
}
