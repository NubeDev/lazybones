import { useState } from "react";
import { Plus } from "lucide-react";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogTrigger,
  DialogClose,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { ApiError } from "@/lib/api/client";
import { useCreateWorkflow } from "@/lib/hooks/use-workflows";
import { WORKTREE_MODES, WorktreeModePicker } from "@/features/tasks/worktree-mode";
import type { WorkspaceDraft } from "@/lib/api/workflows";

const EMPTY: WorkspaceDraft = {
  repo: "",
  base_branch: null,
  branch_prefix: null,
  worktree_mode: "new",
};

/** Create a workflow: id, title, and a workspace block (repo + git config).
 *  Surfaces a `409` inline as "id taken". */
export function WorkflowDialog({
  trigger,
  onCreated,
}: {
  trigger?: React.ReactNode;
  onCreated?: (id: string) => void;
}) {
  const [open, setOpen] = useState(false);
  const [id, setId] = useState("");
  const [title, setTitle] = useState("");
  const [ws, setWs] = useState<WorkspaceDraft>(EMPTY);
  const create = useCreateWorkflow();

  function reset() {
    setId("");
    setTitle("");
    setWs(EMPTY);
  }

  function submit() {
    const tid = id.trim();
    if (!tid || !title.trim() || !ws.repo.trim()) return;
    create.mutate(
      { id: tid, title: title.trim(), workspace: { ...ws, repo: ws.repo.trim() } },
      {
        onSuccess: () => {
          setOpen(false);
          reset();
          onCreated?.(tid);
        },
      },
    );
  }

  const err = create.error;
  const message =
    err instanceof ApiError
      ? err.status === 409
        ? `A workflow "${id.trim()}" already exists.`
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
          create.reset();
          reset();
        }
      }}
    >
      <DialogTrigger asChild>
        {trigger ?? (
          <Button size="sm">
            <Plus /> New workflow
          </Button>
        )}
      </DialogTrigger>
      <DialogContent
        title="New workflow"
        description="A one-off run bound to a repo. Add tasks, then start it."
      >
        <div className="space-y-3">
          <Field label="Workflow id" hint="lowercase, unique, e.g. workflow-1">
            <Input
              value={id}
              autoFocus
              onChange={(e) => setId(e.target.value)}
              placeholder="workflow-1"
              className="font-mono"
            />
          </Field>

          <Field label="Title">
            <Input
              value={title}
              onChange={(e) => setTitle(e.target.value)}
              placeholder="Build the new checkout API"
            />
          </Field>

          <div className="rounded-md border border-border bg-surface-2/40 p-3 space-y-3">
            <span className="text-[11px] font-semibold uppercase tracking-wider text-muted-foreground">
              Workspace
            </span>

            <Field label="Repo" hint="absolute path to the target git repo">
              <Input
                value={ws.repo}
                onChange={(e) => setWs({ ...ws, repo: e.target.value })}
                placeholder="/home/me/code/project"
                className="font-mono"
              />
            </Field>

            <Field label="Base branch" hint="blank = inherit global default">
              <Input
                value={ws.base_branch ?? ""}
                onChange={(e) =>
                  setWs({ ...ws, base_branch: e.target.value.trim() || null })
                }
                placeholder="main"
                className="font-mono"
              />
            </Field>

            <Field label="Branch prefix" hint="blank = inherit global default">
              <Input
                value={ws.branch_prefix ?? ""}
                onChange={(e) =>
                  setWs({ ...ws, branch_prefix: e.target.value.trim() || null })
                }
                placeholder="lazybones/"
                className="font-mono"
              />
            </Field>

            <Field
              label="Worktree mode"
              hint={WORKTREE_MODES[ws.worktree_mode].hint}
            >
              <WorktreeModePicker
                value={ws.worktree_mode}
                onChange={(m) => setWs({ ...ws, worktree_mode: m })}
              />
            </Field>
          </div>
        </div>

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
            disabled={
              !id.trim() || !title.trim() || !ws.repo.trim() || create.isPending
            }
          >
            Create workflow
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  );
}

function Field({
  label,
  hint,
  children,
}: {
  label: string;
  hint?: string;
  children: React.ReactNode;
}) {
  return (
    <label className="block space-y-1">
      <span className="text-xs font-medium">{label}</span>
      {children}
      {hint && <span className="block text-[10px] text-muted-foreground">{hint}</span>}
    </label>
  );
}
