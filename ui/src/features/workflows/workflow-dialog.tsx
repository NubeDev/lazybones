import { useEffect, useState } from "react";
import { Plus, FolderSearch } from "lucide-react";
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
import { useGhRepo, useGhWorktrees } from "@/lib/hooks/use-gh";
import { WorktreeModePicker } from "@/features/tasks/worktree-mode";
import { RepoPicker } from "./repo-picker";
import { BranchField } from "./branch-field";
import { AgentPicker } from "@/features/agents/agent-picker";
import type { WorkspaceDraft } from "@/lib/api/workflows";
import type { WorktreeMode } from "@/types/task";

const EMPTY: WorkspaceDraft = {
  repo: "",
  base_branch: null,
  branch_prefix: null,
  // Default to one-branch-one-PR for the whole workflow (matches the backend
  // default); switch to Isolated for independent parallel tasks.
  worktree_mode: "shared",
  worktree_name: null,
  tool: null,
  model: null,
  effort: null,
  auto_pr: false,
};

/** Modes that create an id-keyed worktree, so a name can override the key. */
const NAMEABLE_MODES = new Set<WorktreeMode>(["new", "shared"]);

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

  // The chosen repo drives the repo-info + branch lookups (null until picked).
  const dir = ws.repo.trim() || null;
  const repo = useGhRepo(dir);

  function reset() {
    setId("");
    setTitle("");
    setWs(EMPTY);
  }

  /** On choosing a repo, record its path and prefill the base branch from the
   *  repo's default (unless the user already typed one). */
  function pickRepo(path: string) {
    setWs((prev) => ({ ...prev, repo: path }));
  }

  // Prefill the base branch from the repo's default once known, but never
  // clobber a value the user set themselves.
  const defaultBranch = repo.data?.default_branch ?? null;
  useEffect(() => {
    if (defaultBranch && !ws.base_branch) {
      setWs((prev) => (prev.base_branch ? prev : { ...prev, base_branch: defaultBranch }));
    }
  }, [defaultBranch, ws.base_branch]);

  function submit() {
    const tid = id.trim();
    if (!tid || !title.trim() || !ws.repo.trim()) return;
    // Normalise a blank name to null (the engine treats "" as unset anyway).
    const name = ws.worktree_name?.trim() || null;
    create.mutate(
      {
        id: tid,
        title: title.trim(),
        workspace: { ...ws, repo: ws.repo.trim(), worktree_name: name },
      },
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

            <Field
              label="Repo"
              hint={
                repo.data
                  ? `${repo.data.full_name} · default ${repo.data.default_branch ?? "?"}`
                  : "absolute path to the target git repo"
              }
            >
              <div className="flex gap-2">
                <Input
                  value={ws.repo}
                  onChange={(e) => setWs({ ...ws, repo: e.target.value })}
                  placeholder="/home/me/code/project"
                  className="font-mono"
                />
                <RepoPicker
                  onPick={pickRepo}
                  trigger={
                    <Button variant="ghost" size="sm" title="Browse for a repo">
                      <FolderSearch /> Browse
                    </Button>
                  }
                />
              </div>
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
                onChange={(m) =>
                  setWs((prev) => ({
                    ...prev,
                    worktree_mode: m,
                    // A name only applies to id-keyed modes; drop it otherwise so
                    // we don't silently send a name that Reuse/Branch ignore.
                    worktree_name: NAMEABLE_MODES.has(m) ? prev.worktree_name : null,
                  }))
                }
              />
            </Field>

            {NAMEABLE_MODES.has(ws.worktree_mode) && (
              <WorktreeNameField
                dir={dir}
                value={ws.worktree_name ?? null}
                onChange={(n) => setWs((prev) => ({ ...prev, worktree_name: n }))}
              />
            )}

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
                  When every task finishes, the default agent writes a summary and opens
                  a GitHub PR for this workflow's branch. Best with Shared mode.
                </span>
              </span>
            </label>
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

/** The trailing path component — a git worktree's on-disk dir name, which is the
 *  key the engine uses (`.lazy/wt/<name>`). */
function baseName(path: string): string {
  const trimmed = path.replace(/\/+$/, "");
  const i = trimmed.lastIndexOf("/");
  return i === -1 ? trimmed : trimmed.slice(i + 1);
}

/**
 * Choose how this workflow's worktree is named: **Auto** (the engine keys it by
 * the workflow/task id — today's behaviour) or **Named**, where you type a fresh
 * name to create a tree or pick an existing one to build in it. The suggestions
 * are the repo's live git worktrees (`GET /gh/worktrees`), so selecting one makes
 * this workflow share that tree. Only shown for the id-keyed modes (Isolated /
 * Shared); Reuse/Main ignore the name.
 */
function WorktreeNameField({
  dir,
  value,
  onChange,
}: {
  dir: string | null;
  value: string | null;
  onChange: (name: string | null) => void;
}) {
  const named = value !== null;
  // Live worktrees under the chosen repo, minus the main checkout (you can't
  // target that as a named worktree). Deduped by dir name.
  const wts = useGhWorktrees(dir);
  const existing = Array.from(
    new Set(
      (wts.data ?? [])
        .filter((w) => !w.is_main)
        .map((w) => baseName(w.path))
        .filter(Boolean),
    ),
  ).sort();
  const matchesExisting = named && existing.includes(value.trim());

  return (
    <Field
      label="Worktree"
      hint={
        named
          ? matchesExisting
            ? "Reuses an existing worktree — this workflow builds in that tree (shared with whatever already uses it)."
            : "Creates a worktree with this name. Reuse the same name on another workflow to share one tree."
          : "Auto: the engine names the worktree after the workflow (Shared) or task (Isolated)."
      }
    >
      <div className="space-y-1.5">
        <div className="flex gap-1 rounded-md border border-border bg-surface p-0.5">
          <button
            type="button"
            onClick={() => onChange(null)}
            className={
              "flex-1 rounded px-2 py-1 text-[11px] font-medium transition-colors " +
              (!named
                ? "bg-accent-soft/60 text-accent"
                : "text-muted-foreground hover:text-foreground")
            }
          >
            Auto
          </button>
          <button
            type="button"
            onClick={() => onChange(value ?? "")}
            className={
              "flex-1 rounded px-2 py-1 text-[11px] font-medium transition-colors " +
              (named
                ? "bg-accent-soft/60 text-accent"
                : "text-muted-foreground hover:text-foreground")
            }
          >
            Name / pick existing
          </button>
        </div>

        {named && (
          <>
            <Input
              list="lazybones-worktree-names"
              value={value}
              onChange={(e) => onChange(e.target.value)}
              placeholder="checkout-api"
              className="font-mono"
            />
            <datalist id="lazybones-worktree-names">
              {existing.map((name) => (
                <option key={name} value={name} />
              ))}
            </datalist>
          </>
        )}
      </div>
    </Field>
  );
}
