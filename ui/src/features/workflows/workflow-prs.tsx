import { useState } from "react";
import {
  GitPullRequest,
  GitPullRequestClosed,
  GitMerge,
  ExternalLink,
  ChevronDown,
  AlertTriangle,
  MessageSquare,
  Plus,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { EmptyState } from "@/components/ui/empty-state";
import { Skeleton } from "@/components/ui/skeleton";
import { Tooltip } from "@/components/ui/tooltip";
import { shortTime, fullTime } from "@/lib/utils/platform";
import {
  DropdownMenu,
  DropdownMenuTrigger,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
} from "@/components/ui/dropdown-menu";
import { ApiError } from "@/lib/api/client";
import {
  useGhAuth,
  useGhPrs,
  useGhPrComments,
  useGhMentionable,
  useCreateGhPr,
  useMergeGhPr,
  useCloseGhPr,
  useCommentGhPr,
} from "@/lib/hooks/use-gh";
import { MentionTextarea } from "./mention-textarea";
import type { GhPullRequest, MergeMethod, PrStateFilter } from "@/types/gh";
import type { Task } from "@/types/task";

const FILTERS: PrStateFilter[] = ["open", "closed", "merged", "all"];

const MERGE_METHODS: { method: MergeMethod; label: string }[] = [
  { method: "merge", label: "Create a merge commit" },
  { method: "squash", label: "Squash and merge" },
  { method: "rebase", label: "Rebase and merge" },
];

/** GitHub pull requests for the workflow's repo, via the user's existing `gh`
 *  login. Lists/filters by state, opens a PR for one of the workflow's task
 *  branches, and lets the user merge (merge/squash/rebase) or close open PRs.
 *
 *  `dir` is the workflow's repo path. `base` is the branch the workflow targets
 *  (the PR base). `tasks` are this workflow's tasks — each ran in its own
 *  worktree on its own branch, and that branch is what a PR opens for. */
export function WorkflowPrs({
  dir,
  base,
  tasks,
}: {
  dir: string;
  base: string | null;
  tasks: Task[];
}) {
  const [filter, setFilter] = useState<PrStateFilter>("open");
  const auth = useGhAuth();
  const { data: prs, isLoading, error } = useGhPrs(dir, filter);
  const { data: mentionable } = useGhMentionable(dir);
  const users = mentionable ?? [];
  const merge = useMergeGhPr();
  const close = useCloseGhPr();
  const [composing, setComposing] = useState(false);
  // Only tasks that actually have a branch can become a PR. A task's branch is
  // created when it claims a worktree, so anything that's run has one.
  const prTasks = tasks.filter((t) => !!t.branch);

  // Not logged in → a clear prompt, not a confusing error.
  if (auth.data && !auth.data.authenticated) {
    return (
      <EmptyState
        icon={GitPullRequestClosed}
        title="GitHub CLI not authenticated"
        description={auth.data.detail ?? "Run `gh auth login`, then reload."}
      />
    );
  }

  return (
    <div className="space-y-3">
      <div className="flex items-center justify-between gap-2">
        <div className="flex gap-1 rounded-md border border-border bg-surface p-0.5">
          {FILTERS.map((f) => (
            <button
              key={f}
              onClick={() => setFilter(f)}
              className={
                "rounded px-2.5 py-1 text-xs capitalize transition-colors " +
                (filter === f
                  ? "bg-muted text-foreground"
                  : "text-muted-foreground hover:text-foreground")
              }
            >
              {f}
            </button>
          ))}
        </div>
        {prTasks.length > 0 && (
          <Button size="sm" onClick={() => setComposing((v) => !v)}>
            <Plus /> New PR
          </Button>
        )}
      </div>

      {composing && prTasks.length > 0 && (
        <NewPrForm
          dir={dir}
          base={base}
          tasks={prTasks}
          users={users}
          onDone={() => {
            setComposing(false);
            setFilter("open");
          }}
        />
      )}

      {merge.error && (
        <p className="rounded-md border border-status-blocked/40 bg-status-blocked/10 px-3 py-2 text-xs text-status-blocked">
          {merge.error instanceof ApiError
            ? merge.error.message
            : "Could not merge the pull request."}
        </p>
      )}

      {isLoading && <Skeleton className="h-24 w-full" />}

      {error && (
        <EmptyState
          icon={GitPullRequestClosed}
          title="Can't load pull requests"
          description={
            error instanceof ApiError ? error.message : "Unexpected error"
          }
        />
      )}

      {prs && prs.length === 0 && !isLoading && (
        <EmptyState
          icon={GitPullRequest}
          title={`No ${filter === "all" ? "" : filter} pull requests`}
          description="Nothing to show for this repo and filter."
        />
      )}

      <ul className="divide-y divide-border rounded-md border border-border">
        {prs?.map((pr) => (
          <PrRow
            key={pr.number}
            dir={dir}
            pr={pr}
            users={users}
            mergePending={merge.isPending}
            closePending={close.isPending}
            onMerge={(method) =>
              merge.mutate({ dir, number: pr.number, method })
            }
            onClose={() => close.mutate({ dir, number: pr.number })}
          />
        ))}
      </ul>
    </div>
  );
}

/** Inline composer for opening a PR from the workflow's task branches.
 *
 *  Two scopes:
 *   • **Whole workflow** (default) — open ONE PR for everything the workflow did.
 *     lazybones lands each finished task onto the next, so the *last* task's
 *     branch is the tip of the stack and contains all the work; that single
 *     branch → base is the whole feature in one PR. (For a `shared`-mode workflow
 *     there's literally one branch, so this is exactly it.)
 *   • **Single task** — open a PR for just one task's branch, for when you really
 *     do want to review tasks separately.
 *
 *  Either way `gh` runs from the chosen task's worktree so it resolves the right
 *  remote + branch. The branch must already be on the remote (the scheduler
 *  pushes task branches); otherwise `gh` errors clearly and we surface it. */
function NewPrForm({
  dir,
  base,
  tasks,
  users,
  onDone,
}: {
  dir: string;
  base: string | null;
  tasks: Task[];
  users: string[];
  onDone: () => void;
}) {
  const create = useCreateGhPr();
  // The stack tip = the last task that has a branch. With sequential landing,
  // its branch carries every prior task's commits, so it's the whole-workflow
  // head. `tasks` arrives in workflow order.
  const tip = [...tasks].reverse().find((t) => !!t.branch) ?? tasks[0];
  // One distinct branch across all tasks ⇒ a shared-mode workflow; "per task"
  // would be meaningless, so we hide that toggle.
  const distinctBranches = new Set(tasks.map((t) => t.branch).filter(Boolean));
  const stacked = distinctBranches.size > 1;

  const [scope, setScope] = useState<"workflow" | "task">("workflow");
  const [taskId, setTaskId] = useState(tip?.id ?? "");
  const selected = tasks.find((t) => t.id === taskId) ?? tip;
  // The task whose branch + worktree the PR actually uses.
  const head = scope === "workflow" ? tip : selected;

  const [title, setTitle] = useState(tip?.title ?? "");
  const [body, setBody] = useState("");
  const [draft, setDraft] = useState(false);

  // The PR targets the workflow's base branch. Falls back to "master" only if
  // the workflow never recorded one.
  const baseBranch = base ?? "master";

  function pickScope(s: "workflow" | "task") {
    setScope(s);
    // Re-seed the title to match the new head, unless the user typed their own.
    const h = s === "workflow" ? tip : selected;
    if (h && (!title.trim() || title === tip?.title || title === selected?.title))
      setTitle(h.title);
  }

  function pickTask(id: string) {
    setTaskId(id);
    const t = tasks.find((x) => x.id === id);
    if (t && (!title.trim() || title === selected?.title)) setTitle(t.title);
  }

  function submit() {
    const t = title.trim();
    if (!t || !head?.branch) return;
    create.mutate(
      {
        // Always run `gh` in the workflow's main repo, NOT the task worktree:
        // worktrees are torn down once a task finishes, so head.worktree often
        // points at a deleted dir — and `gh` spawned there fails with a
        // misleading "No such file or directory" (it's the cwd that's gone, not
        // gh). The branch lives on the remote, which the main repo resolves fine.
        dir,
        title: t,
        body: body.trim(),
        head: head.branch,
        base: baseBranch,
        draft,
      },
      {
        onSuccess: () => {
          setBody("");
          onDone();
        },
      },
    );
  }

  return (
    <div className="space-y-2 rounded-md border border-border bg-surface-2/40 p-3">
      {stacked && (
        <div className="flex gap-1 rounded-md border border-border bg-surface p-0.5 text-[11px]">
          {(
            [
              ["workflow", "Whole workflow (one PR)"],
              ["task", "Single task"],
            ] as const
          ).map(([s, label]) => (
            <button
              key={s}
              type="button"
              onClick={() => pickScope(s)}
              className={
                "flex-1 rounded px-2 py-1 font-medium transition-colors " +
                (scope === s
                  ? "bg-accent-soft/60 text-accent"
                  : "text-muted-foreground hover:text-foreground")
              }
            >
              {label}
            </button>
          ))}
        </div>
      )}

      {scope === "task" && stacked && (
        <label className="flex flex-col gap-1 text-xs text-muted-foreground">
          Task
          <select
            value={taskId}
            onChange={(e) => pickTask(e.target.value)}
            className="rounded border border-border bg-surface px-2 py-1 text-sm text-foreground"
          >
            {tasks
              .filter((t) => !!t.branch)
              .map((t) => (
                <option key={t.id} value={t.id}>
                  {t.title} ({t.branch})
                </option>
              ))}
          </select>
        </label>
      )}

      {head?.branch && (
        <p className="font-mono text-[11px] text-muted-foreground">
          {head.branch} → {baseBranch}
          {scope === "workflow" && stacked && (
            <span className="ml-1.5 font-sans not-italic text-muted-foreground/70">
              · all {distinctBranches.size} task branches, stacked
            </span>
          )}
        </p>
      )}

      <Input
        value={title}
        autoFocus
        onChange={(e) => setTitle(e.target.value)}
        placeholder="Pull request title"
      />
      <MentionTextarea
        value={body}
        onChange={setBody}
        users={users}
        placeholder="Description (optional) — type @ to mention"
        rows={3}
      />
      <label className="flex items-center gap-1.5 text-xs text-muted-foreground">
        <input
          type="checkbox"
          checked={draft}
          onChange={(e) => setDraft(e.target.checked)}
        />
        Open as draft
      </label>
      <div className="flex justify-end gap-2">
        <Button variant="ghost" size="sm" onClick={onDone}>
          Cancel
        </Button>
        <Button
          size="sm"
          onClick={submit}
          disabled={!title.trim() || !head?.branch || create.isPending}
        >
          Create PR
        </Button>
      </div>
      {create.error && (
        <p className="text-[11px] text-status-blocked">
          {create.error instanceof ApiError
            ? create.error.message
            : "Could not open the pull request."}
        </p>
      )}
    </div>
  );
}

function PrRow({
  dir,
  pr,
  users,
  mergePending,
  closePending,
  onMerge,
  onClose,
}: {
  dir: string;
  pr: GhPullRequest;
  users: string[];
  mergePending: boolean;
  closePending: boolean;
  onMerge: (method: MergeMethod) => void;
  onClose: () => void;
}) {
  const state = pr.state.toUpperCase();
  const open = state === "OPEN";
  const merged = state === "MERGED";
  const [expanded, setExpanded] = useState(false);
  // GitHub only computes mergeability for open PRs; treat anything but a clear
  // CONFLICTING as merge-able so we don't block on an UNKNOWN that's still
  // resolving server-side.
  const conflicting = pr.mergeable.toUpperCase() === "CONFLICTING";

  const Icon = merged
    ? GitMerge
    : open
      ? GitPullRequest
      : GitPullRequestClosed;
  const iconColor = merged
    ? "text-accent"
    : open
      ? "text-status-running"
      : "text-muted-foreground";

  return (
    <li className="px-3 py-2.5">
      <div className="flex items-start gap-3">
      <Icon className={`mt-0.5 size-4 shrink-0 ${iconColor}`} />
      <div className="min-w-0 flex-1">
        <div className="flex items-center gap-2">
          <span className="truncate text-sm font-medium">{pr.title}</span>
          <span className="shrink-0 text-xs text-muted-foreground">
            #{pr.number}
          </span>
          {pr.is_draft && (
            <span className="shrink-0 rounded-full border border-border px-1.5 py-px text-[10px] text-muted-foreground">
              draft
            </span>
          )}
        </div>
        <div className="mt-1 flex flex-wrap items-center gap-1.5">
          {pr.head_ref && (
            <span className="font-mono text-[11px] text-muted-foreground">
              {pr.head_ref} → {pr.base_ref}
            </span>
          )}
          {pr.author && (
            <span className="text-[11px] text-muted-foreground">
              by {pr.author}
            </span>
          )}
          {/* The lifecycle timestamp most relevant to the current state:
              merged-at for merged PRs, closed-at for closed, opened-at for open.
              Hover for the full date/time/zone. */}
          {(() => {
            const verb = merged ? "merged" : open ? "opened" : "closed";
            const at = merged
              ? pr.merged_at
              : open
                ? pr.created_at
                : (pr.closed_at ?? pr.created_at);
            if (!at) return null;
            return (
              <Tooltip label={fullTime(at)} side="bottom">
                <span className="text-[11px] text-muted-foreground">
                  {verb} {shortTime(at)}
                </span>
              </Tooltip>
            );
          })()}
          {open && conflicting && (
            <span className="inline-flex items-center gap-1 text-[11px] text-status-blocked">
              <AlertTriangle className="size-3" /> conflicts
            </span>
          )}
          {pr.labels.map((l) => (
            <span
              key={l}
              className="rounded-full border border-border px-1.5 py-px text-[10px] text-muted-foreground"
            >
              {l}
            </span>
          ))}
        </div>
      </div>
      <div className="flex shrink-0 items-center gap-1">
        <Button
          variant="ghost"
          size="sm"
          onClick={() => setExpanded((v) => !v)}
          title="Comments"
        >
          <MessageSquare className="size-3.5" /> Comments
        </Button>
        <a
          href={pr.url}
          target="_blank"
          rel="noreferrer"
          className="rounded p-1 text-muted-foreground hover:text-foreground"
          title="Open on GitHub"
        >
          <ExternalLink className="size-3.5" />
        </a>
        {open && (
          <>
            <Button
              variant="ghost"
              size="sm"
              disabled={closePending || mergePending}
              onClick={onClose}
            >
              Close
            </Button>
            <DropdownMenu>
              <DropdownMenuTrigger asChild>
                <Button
                  size="sm"
                  disabled={mergePending || pr.is_draft || conflicting}
                  title={
                    pr.is_draft
                      ? "Draft PRs can't be merged"
                      : conflicting
                        ? "Resolve conflicts before merging"
                        : "Merge this pull request"
                  }
                >
                  <GitMerge /> Merge <ChevronDown className="size-3" />
                </Button>
              </DropdownMenuTrigger>
              <DropdownMenuContent>
                <DropdownMenuLabel>Merge method</DropdownMenuLabel>
                {MERGE_METHODS.map((m) => (
                  <DropdownMenuItem
                    key={m.method}
                    onSelect={() => onMerge(m.method)}
                  >
                    {m.label}
                  </DropdownMenuItem>
                ))}
              </DropdownMenuContent>
            </DropdownMenu>
          </>
        )}
      </div>
      </div>

      {expanded && (
        <PrComments dir={dir} number={pr.number} users={users} />
      )}
    </li>
  );
}

function PrComments({
  dir,
  number,
  users,
}: {
  dir: string;
  number: number;
  users: string[];
}) {
  const { data: comments, isLoading, error } = useGhPrComments(dir, number);
  const comment = useCommentGhPr();
  const [draft, setDraft] = useState("");

  function submit() {
    const b = draft.trim();
    if (!b) return;
    comment.mutate(
      { dir, number, body: b },
      { onSuccess: () => setDraft("") },
    );
  }

  return (
    <div className="ml-7 mt-3 space-y-3 border-l border-border pl-3">
      {isLoading && <Skeleton className="h-10 w-full" />}

      {error && (
        <p className="text-xs text-status-blocked">
          {error instanceof ApiError ? error.message : "Can't load comments."}
        </p>
      )}

      {comments && comments.length === 0 && !isLoading && (
        <p className="text-xs text-muted-foreground">No comments yet.</p>
      )}

      {comments?.map((c, i) => (
        <div key={c.url || i} className="space-y-1">
          <div className="flex items-center gap-2 text-[11px] text-muted-foreground">
            <span className="font-medium text-foreground">
              {c.author ?? "unknown"}
            </span>
            {c.created_at && (
              <Tooltip label={fullTime(c.created_at)} side="bottom">
                <span>{shortTime(c.created_at)}</span>
              </Tooltip>
            )}
          </div>
          <p className="whitespace-pre-wrap text-sm text-foreground/90">
            {c.body}
          </p>
        </div>
      ))}

      <div className="space-y-2">
        <MentionTextarea
          value={draft}
          onChange={setDraft}
          users={users}
          placeholder="Add a comment… type @ to mention"
          rows={2}
        />
        <div className="flex items-center justify-end gap-2">
          {comment.error && (
            <span className="text-[11px] text-status-blocked">
              {comment.error instanceof ApiError
                ? comment.error.message
                : "Could not comment."}
            </span>
          )}
          <Button
            size="sm"
            onClick={submit}
            disabled={!draft.trim() || comment.isPending}
          >
            Comment
          </Button>
        </div>
      </div>
    </div>
  );
}
