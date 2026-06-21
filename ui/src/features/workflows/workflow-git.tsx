import { useMemo, useState } from "react";
import {
  Check,
  FolderGit2,
  GitBranch,
  GitCompare,
  GitMerge,
  Lock,
  Plus,
  Trash2,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Tooltip } from "@/components/ui/tooltip";
import { EmptyState } from "@/components/ui/empty-state";
import { Skeleton } from "@/components/ui/skeleton";
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogTrigger,
} from "@/components/ui/dialog";
import { ApiError } from "@/lib/api/client";
import {
  useCheckoutGhBranch,
  useCreateGhBranch,
  useDeleteGhBranch,
  useGhLocalBranches,
  useGhWorktrees,
  usePruneGhWorktrees,
  useRemoveGhWorktree,
} from "@/lib/hooks/use-gh";
import { useFileDiff } from "@/lib/hooks/use-files";
import type { Task } from "@/types/task";
import { repoBasename } from "./repo-path";
import { DiffView } from "./diff-view";

/** A task whose worktree is checked out is "in use" and must not be torn down. */
const IN_USE: ReadonlySet<Task["status"]> = new Set(["running", "gating"]);

function errMsg(e: unknown, fallback: string): string {
  return e instanceof ApiError ? e.message : fallback;
}

/** In-workflow Git manager: inspect + safely manage the repo's branches and the
 *  worktrees lazybones created. Operates on the workflow's fixed repo (`dir`)
 *  using *local* git (no GitHub remote or `gh` auth required). `tasks` are this
 *  workflow's tasks, used to mark worktrees in use. */
export function WorkflowGit({
  dir,
  base,
  tasks,
}: {
  dir: string;
  base: string | null;
  tasks: Task[];
}) {
  // The repo is fixed by the workspace. If it's unset we can't do anything —
  // say so plainly instead of firing failing git calls.
  if (!dir.trim()) {
    return (
      <EmptyState
        icon={FolderGit2}
        title="No repository configured"
        description="This workflow's workspace has no repo path set, so there's nothing to manage here."
      />
    );
  }

  // The main worktree's branch is the repo's actual checked-out HEAD — more
  // accurate than the repo's *default* branch for marking "current".
  const worktrees = useGhWorktrees(dir);
  const current = worktrees.data?.find((w) => w.is_main)?.branch ?? null;

  return (
    <div className="space-y-4">
      {/* Make it unambiguous which checkout we're operating on. */}
      <Tooltip label={dir} side="bottom">
        <p className="inline-flex items-center gap-1.5 text-xs text-muted-foreground">
          <FolderGit2 className="size-3.5" />
          Operating on <span className="font-mono text-foreground">{dir}</span>
        </p>
      </Tooltip>
      <BranchesCard dir={dir} current={current} />
      <DiffCard dir={dir} base={base} />
      <WorktreesCard dir={dir} tasks={tasks} />
    </div>
  );
}

// ---- Card: repo-wide diff (uncommitted ↔ branch-vs-base) ----------------

function DiffCard({ dir, base }: { dir: string; base: string | null }) {
  // null ⇒ uncommitted working changes; base name ⇒ branch-vs-base.
  const [scope, setScope] = useState<string | null>(null);
  const { data, isLoading, error } = useFileDiff(dir, scope, null);

  return (
    <section className="rounded-lg border border-border bg-surface p-4">
      <header className="mb-3 flex items-center justify-between gap-2">
        <h3 className="inline-flex items-center gap-1.5 text-sm font-medium">
          <GitCompare className="size-4 text-muted-foreground" /> Diff
        </h3>
        <div className="flex items-center gap-1">
          <ScopeChip
            active={scope === null}
            onClick={() => setScope(null)}
            label="Uncommitted"
          />
          {base && (
            <ScopeChip
              active={scope === base}
              onClick={() => setScope(base)}
              label={`vs ${base}`}
            />
          )}
        </div>
      </header>

      {isLoading && <Skeleton className="h-40 w-full" />}
      {error && (
        <p className="rounded-md bg-status-blocked/10 px-3 py-2 text-xs text-status-blocked">
          {errMsg(error, "Can't compute diff.")}
        </p>
      )}
      {data && data.diff.trim() === "" && !isLoading && (
        <p className="py-6 text-center text-xs text-muted-foreground">
          No changes{scope ? ` against ${scope}` : ""}.
        </p>
      )}
      {data && data.diff.trim() !== "" && (
        <div className="max-h-[60vh]">
          <DiffView diff={data.diff} />
        </div>
      )}
    </section>
  );
}

/** A pill toggle for which diff scope to show. */
function ScopeChip({
  active,
  onClick,
  label,
}: {
  active: boolean;
  onClick: () => void;
  label: string;
}) {
  return (
    <button
      onClick={onClick}
      className={`rounded-full border px-2 py-0.5 text-[11px] ${
        active
          ? "border-accent bg-accent/10 text-accent"
          : "border-border text-muted-foreground hover:bg-surface-2/60"
      }`}
    >
      {label}
    </button>
  );
}

// ---- Card A: branches ---------------------------------------------------

function BranchesCard({ dir, current }: { dir: string; current: string | null }) {
  const { data: branches, isLoading, error } = useGhLocalBranches(dir);
  const create = useCreateGhBranch();
  const checkout = useCheckoutGhBranch();
  const del = useDeleteGhBranch();

  const [adding, setAdding] = useState(false);
  const [name, setName] = useState("");
  const [from, setFrom] = useState("");

  const nameValid = name.trim().length > 0 && !/\s/.test(name);

  function submitNew() {
    if (!nameValid) return;
    create.mutate(
      { dir, name: name.trim(), from: from.trim() || null },
      {
        onSuccess: () => {
          setAdding(false);
          setName("");
          setFrom("");
        },
      },
    );
  }

  return (
    <section className="rounded-lg border border-border bg-surface p-4">
      <header className="mb-3 flex items-center justify-between">
        <h3 className="inline-flex items-center gap-1.5 text-sm font-medium">
          <GitBranch className="size-4 text-muted-foreground" /> Branches
        </h3>
        <Button size="sm" onClick={() => setAdding((v) => !v)}>
          <Plus /> New branch
        </Button>
      </header>

      {adding && (
        <div className="mb-3 space-y-2 rounded-md border border-border bg-surface-2/40 p-3">
          <Input
            value={name}
            autoFocus
            onChange={(e) => setName(e.target.value)}
            placeholder="branch name (no spaces)"
            className="font-mono"
          />
          <Input
            value={from}
            onChange={(e) => setFrom(e.target.value)}
            placeholder={`from (defaults to ${current ?? "current"})`}
            className="font-mono"
          />
          <div className="flex justify-end gap-2">
            <Button variant="ghost" size="sm" onClick={() => setAdding(false)}>
              Cancel
            </Button>
            <Button
              size="sm"
              onClick={submitNew}
              disabled={!nameValid || create.isPending}
            >
              Create &amp; switch
            </Button>
          </div>
          {create.error && (
            <p className="text-xs text-status-blocked">
              {errMsg(create.error, "Could not create branch.")}
            </p>
          )}
        </div>
      )}

      {isLoading && <Skeleton className="h-20 w-full" />}
      {error && (
        <p className="rounded-md bg-status-blocked/10 px-3 py-2 text-xs text-status-blocked">
          {errMsg(error, "Can't load branches.")}
        </p>
      )}
      {branches && branches.length === 0 && !isLoading && (
        <p className="text-xs text-muted-foreground">No local branches yet.</p>
      )}

      {branches && branches.length > 0 && (
        <ul className="divide-y divide-border rounded-md border border-border">
          {branches.map((b) => {
            const isCurrent = b.name === current;
            const switching =
              checkout.isPending && checkout.variables?.branch === b.name;
            const deleting = del.isPending && del.variables?.name === b.name;
            return (
              <li
                key={b.name}
                className="flex items-center gap-2 px-3 py-2 text-xs"
              >
                <span className="truncate font-mono">{b.name}</span>
                <span className="font-mono text-muted-foreground/70">
                  {b.sha}
                </span>
                {isCurrent && (
                  <span className="rounded-full bg-accent/15 px-1.5 py-px text-[10px] text-accent">
                    current
                  </span>
                )}
                {b.ahead > 0 && (
                  <Tooltip
                    label={`${b.ahead} commit(s) ahead of ${b.upstream}`}
                    side="top"
                  >
                    <span className="text-[10px] text-muted-foreground">
                      ↑{b.ahead}
                    </span>
                  </Tooltip>
                )}
                {b.behind > 0 && (
                  <Tooltip
                    label={`${b.behind} commit(s) behind ${b.upstream}`}
                    side="top"
                  >
                    <span className="text-[10px] text-muted-foreground">
                      ↓{b.behind}
                    </span>
                  </Tooltip>
                )}
                <div className="ml-auto flex items-center gap-1">
                  <Button
                    variant="ghost"
                    size="sm"
                    disabled={isCurrent || switching}
                    onClick={() => checkout.mutate({ dir, branch: b.name })}
                  >
                    {switching ? "Switching…" : "Switch"}
                  </Button>
                  <DeleteBranchButton
                    disabled={isCurrent || deleting}
                    pending={deleting}
                    branch={b.name}
                    error={
                      del.variables?.name === b.name && del.error
                        ? errMsg(del.error, "Delete failed.")
                        : null
                    }
                    onConfirm={(force) =>
                      del.mutate({ dir, name: b.name, force })
                    }
                  />
                </div>
              </li>
            );
          })}
        </ul>
      )}
    </section>
  );
}

/** Destructive branch delete: confirm dialog, with a force retry offered after a
 *  failed plain `-d` (unmerged work). */
function DeleteBranchButton({
  branch,
  disabled,
  pending,
  error,
  onConfirm,
}: {
  branch: string;
  disabled: boolean;
  pending: boolean;
  error: string | null;
  onConfirm: (force: boolean) => void;
}) {
  return (
    <Dialog>
      <DialogTrigger asChild>
        <Button
          variant="ghost"
          size="sm"
          disabled={disabled}
          className="text-status-blocked hover:bg-status-blocked/10 hover:text-status-blocked"
        >
          {pending ? "Deleting…" : <Trash2 />}
        </Button>
      </DialogTrigger>
      <DialogContent
        title="Delete branch?"
        description={`${branch} will be deleted from the local repo. This can't be undone.`}
      >
        <div className="space-y-3">
          {error && (
            <p className="rounded-md bg-status-blocked/10 px-2 py-1.5 text-xs text-status-blocked">
              {error}
            </p>
          )}
          <div className="flex justify-end gap-2">
            <DialogClose asChild>
              <Button variant="ghost" size="sm">
                Cancel
              </Button>
            </DialogClose>
            <Button
              variant="destructive"
              size="sm"
              onClick={() => onConfirm(false)}
            >
              Delete
            </Button>
            {error && (
              <Button
                variant="destructive"
                size="sm"
                onClick={() => onConfirm(true)}
              >
                Force delete
              </Button>
            )}
          </div>
        </div>
      </DialogContent>
    </Dialog>
  );
}

// ---- Card B: worktrees --------------------------------------------------

function WorktreesCard({ dir, tasks }: { dir: string; tasks: Task[] }) {
  const { data: worktrees, isLoading, error } = useGhWorktrees(dir);
  const remove = useRemoveGhWorktree();
  const prune = usePruneGhWorktrees();

  // path → owning task, for cross-referencing.
  const byPath = useMemo(() => {
    const m = new Map<string, Task>();
    for (const t of tasks) if (t.worktree) m.set(t.worktree, t);
    return m;
  }, [tasks]);

  return (
    <section className="rounded-lg border border-border bg-surface p-4">
      <header className="mb-3 flex items-center justify-between">
        <h3 className="inline-flex items-center gap-1.5 text-sm font-medium">
          <FolderGit2 className="size-4 text-muted-foreground" /> Worktrees
        </h3>
      </header>

      {isLoading && <Skeleton className="h-20 w-full" />}
      {error && (
        <p className="text-xs text-status-blocked">
          {errMsg(error, "Can't load worktrees.")}
        </p>
      )}
      {worktrees && worktrees.length === 0 && !isLoading && (
        <p className="text-xs text-muted-foreground">No worktrees.</p>
      )}

      <ul className="divide-y divide-border rounded-md border border-border">
        {worktrees?.map((w) => {
          const owner = byPath.get(w.path);
          const inUse = owner ? IN_USE.has(owner.status) : false;
          const removing =
            remove.isPending && remove.variables?.path === w.path;
          return (
            <li
              key={w.path}
              className="flex items-center gap-2 px-3 py-2 text-xs"
            >
              <Tooltip label={w.path} side="bottom">
                <span className="truncate font-mono">{repoBasename(w.path)}</span>
              </Tooltip>
              {w.branch && (
                <span className="inline-flex items-center gap-1 text-muted-foreground">
                  <GitMerge className="size-3" />
                  <span className="font-mono">{w.branch}</span>
                </span>
              )}
              {w.head && (
                <span className="font-mono text-muted-foreground/70">
                  {w.head.slice(0, 7)}
                </span>
              )}
              {w.is_main && (
                <span className="rounded-full border border-border px-1.5 py-px text-[10px] text-muted-foreground">
                  main
                </span>
              )}
              {w.locked && (
                <span className="inline-flex items-center gap-0.5 rounded-full border border-border px-1.5 py-px text-[10px] text-muted-foreground">
                  <Lock className="size-2.5" /> locked
                </span>
              )}
              {owner && (
                <span className="rounded-full bg-muted px-1.5 py-px font-mono text-[10px] text-muted-foreground">
                  {owner.id}
                </span>
              )}
              <div className="ml-auto flex items-center gap-1">
                {w.is_main ? (
                  <span className="px-2 py-1 text-[10px] text-muted-foreground/60">
                    primary
                  </span>
                ) : inUse ? (
                  <Tooltip label="in use by running task" side="left">
                    <span className="px-2 py-1 text-[10px] text-muted-foreground/60">
                      <Check className="inline size-3" /> in use
                    </span>
                  </Tooltip>
                ) : (
                  <RemoveWorktreeButton
                    path={w.path}
                    pending={removing}
                    error={
                      remove.variables?.path === w.path && remove.error
                        ? errMsg(remove.error, "Remove failed.")
                        : null
                    }
                    onConfirm={(force) =>
                      remove.mutate({ dir, path: w.path, force })
                    }
                  />
                )}
              </div>
            </li>
          );
        })}
      </ul>

      <div className="mt-3 flex items-center justify-end">
        <Button
          variant="outline"
          size="sm"
          disabled={prune.isPending}
          onClick={() => prune.mutate({ dir })}
        >
          {prune.isPending ? "Pruning…" : "Prune stale"}
        </Button>
      </div>
      {prune.error && (
        <p className="mt-2 text-right text-xs text-status-blocked">
          {errMsg(prune.error, "Prune failed.")}
        </p>
      )}
    </section>
  );
}

/** Destructive worktree remove: confirm dialog + force retry on failure. */
function RemoveWorktreeButton({
  path,
  pending,
  error,
  onConfirm,
}: {
  path: string;
  pending: boolean;
  error: string | null;
  onConfirm: (force: boolean) => void;
}) {
  return (
    <Dialog>
      <DialogTrigger asChild>
        <Button
          variant="ghost"
          size="sm"
          disabled={pending}
          className="text-status-blocked hover:bg-status-blocked/10 hover:text-status-blocked"
        >
          {pending ? "Removing…" : <Trash2 />}
        </Button>
      </DialogTrigger>
      <DialogContent
        title="Remove worktree?"
        description={`${path} will be detached from the repo. Uncommitted changes in it would be lost.`}
      >
        <div className="space-y-3">
          {error && (
            <p className="rounded-md bg-status-blocked/10 px-2 py-1.5 text-xs text-status-blocked">
              {error}
            </p>
          )}
          <div className="flex justify-end gap-2">
            <DialogClose asChild>
              <Button variant="ghost" size="sm">
                Cancel
              </Button>
            </DialogClose>
            <Button
              variant="destructive"
              size="sm"
              onClick={() => onConfirm(false)}
            >
              Remove
            </Button>
            {error && (
              <Button
                variant="destructive"
                size="sm"
                onClick={() => onConfirm(true)}
              >
                Force remove
              </Button>
            )}
          </div>
        </div>
      </DialogContent>
    </Dialog>
  );
}
