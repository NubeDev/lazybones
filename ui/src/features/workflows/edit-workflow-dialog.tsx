import { useState } from "react";
import { Settings2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogTrigger,
  DialogClose,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { ApiError } from "@/lib/api/client";
import { useUpdateWorkflow } from "@/lib/hooks/use-workflows";
import { WorktreeModePicker } from "@/features/tasks/worktree-mode";
import { BranchField } from "./branch-field";
import { AgentPicker } from "@/features/agents/agent-picker";
import type { WorkspaceDraft } from "@/lib/api/workflows";
import type { WorkflowDetail } from "@/types/workflow";

/** Edit a workflow's workspace defaults — the agent triple (tool/model/effort)
 *  plus the git config its tasks inherit (base branch, branch prefix, worktree
 *  mode). The `repo` is fixed after creation (re-pointing a live workflow would
 *  orphan its worktrees), so it's shown read-only. Tasks pick up the new defaults
 *  on their next claim; a task that pins its own value still wins. */
export function EditWorkflowDialog({
  workflow,
  trigger,
  open: openProp,
  onOpenChange,
}: {
  workflow: WorkflowDetail;
  trigger?: React.ReactNode;
  /** Controlled-open (e.g. opened from a menu item). Omit for self-managed. */
  open?: boolean;
  onOpenChange?: (open: boolean) => void;
}) {
  const [openState, setOpenState] = useState(false);
  // Controlled when `open` is supplied; otherwise self-managed.
  const open = openProp ?? openState;
  const setOpen = (o: boolean) => {
    setOpenState(o);
    onOpenChange?.(o);
  };

  return (
    <Dialog open={open} onOpenChange={setOpen}>
      {/* No trigger in controlled mode — the parent opens it. */}
      {openProp === undefined && (
        <DialogTrigger asChild>
          {trigger ?? (
            <Button size="sm" variant="ghost">
              <Settings2 /> Edit workflow
            </Button>
          )}
        </DialogTrigger>
      )}
      <DialogContent
        title={`Edit ${workflow.id}`}
        description="Change the agent and git defaults this workflow's tasks inherit. A task that pins its own value still wins; running work is undisturbed."
        className="max-h-[85vh] overflow-y-auto"
      >
        {/* Remount on open so the form always reflects the latest saved state. */}
        {open && <EditBody workflow={workflow} onDone={() => setOpen(false)} />}
      </DialogContent>
    </Dialog>
  );
}

function EditBody({
  workflow,
  onDone,
}: {
  workflow: WorkflowDetail;
  onDone: () => void;
}) {
  const ws0 = workflow.workspace;
  const [ws, setWs] = useState<WorkspaceDraft>({
    repo: ws0.repo,
    base_branch: ws0.base_branch,
    branch_prefix: ws0.branch_prefix,
    worktree_mode: ws0.worktree_mode,
    tool: ws0.tool,
    model: ws0.model,
    effort: ws0.effort,
    auto_pr: ws0.auto_pr ?? false,
  });

  const update = useUpdateWorkflow();
  const dir = ws.repo.trim() || null;

  function submit() {
    update.mutate(
      { id: workflow.id, workspace: ws },
      { onSuccess: () => onDone() },
    );
  }

  const err = update.error;
  const message =
    err instanceof ApiError
      ? err.status === 404
        ? "This workflow no longer exists."
        : err.message
      : err
        ? "Something went wrong."
        : null;

  return (
    <>
      <div className="space-y-3">
        <Field label="Repo" hint="fixed after creation">
          <Input value={ws.repo} disabled className="font-mono" />
        </Field>

        <Field label="Base branch" hint="blank = inherit global default">
          <BranchField
            dir={dir}
            value={ws.base_branch}
            onChange={(b) => setWs((prev) => ({ ...prev, base_branch: b }))}
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

        <Field label="Worktree mode">
          <WorktreeModePicker
            value={ws.worktree_mode}
            onChange={(m) => setWs({ ...ws, worktree_mode: m })}
          />
        </Field>

        <AgentPicker
          tool={ws.tool ?? ""}
          model={ws.model}
          effort={ws.effort}
          onToolChange={(t) => setWs((prev) => ({ ...prev, tool: t.trim() || null }))}
          onModelChange={(m) => setWs((prev) => ({ ...prev, model: m }))}
          onEffortChange={(e) => setWs((prev) => ({ ...prev, effort: e }))}
          labels={{ agent: "Default agent", agentHint: "blank = inherit global default" }}
        />

        <label className="flex cursor-pointer items-start gap-2 text-xs">
          <input
            type="checkbox"
            className="mt-0.5"
            checked={ws.auto_pr ?? false}
            onChange={(e) => setWs((prev) => ({ ...prev, auto_pr: e.target.checked }))}
          />
          <span>
            <b className="font-medium">Auto-create PR when done</b>
            <span className="block text-muted-foreground">
              When every task finishes, the default agent writes a summary and opens a
              GitHub PR for this workflow's branch. Best with Shared mode.
            </span>
          </span>
        </label>
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
        <Button size="sm" onClick={submit} disabled={update.isPending}>
          {update.isPending ? "Saving…" : "Save changes"}
        </Button>
      </div>
    </>
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
