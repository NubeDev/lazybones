import { useState } from "react";
import {
  CircleDot,
  CircleCheck,
  ExternalLink,
  Plus,
  GitPullRequestClosed,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { EmptyState } from "@/components/ui/empty-state";
import { Skeleton } from "@/components/ui/skeleton";
import { ApiError } from "@/lib/api/client";
import {
  useGhAuth,
  useGhIssues,
  useCreateGhIssue,
  useCloseGhIssue,
} from "@/lib/hooks/use-gh";
import type { IssueStateFilter } from "@/types/gh";

const FILTERS: IssueStateFilter[] = ["open", "closed", "all"];

/** GitHub issues for the workflow's repo, via the user's existing `gh` login.
 *  Lists/filters by state, opens a new issue, and closes open ones. `dir` is the
 *  workflow's repo path. */
export function WorkflowIssues({ dir }: { dir: string }) {
  const [filter, setFilter] = useState<IssueStateFilter>("open");
  const auth = useGhAuth();
  const { data: issues, isLoading, error } = useGhIssues(dir, filter);
  const create = useCreateGhIssue();
  const close = useCloseGhIssue();
  const [composing, setComposing] = useState(false);
  const [title, setTitle] = useState("");
  const [body, setBody] = useState("");

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

  function submitNew() {
    const t = title.trim();
    if (!t) return;
    create.mutate(
      { dir, title: t, body: body.trim() },
      {
        onSuccess: () => {
          setComposing(false);
          setTitle("");
          setBody("");
          setFilter("open");
        },
      },
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
        <Button size="sm" onClick={() => setComposing((v) => !v)}>
          <Plus /> New issue
        </Button>
      </div>

      {composing && (
        <div className="space-y-2 rounded-md border border-border bg-surface-2/40 p-3">
          <Input
            value={title}
            autoFocus
            onChange={(e) => setTitle(e.target.value)}
            placeholder="Issue title"
          />
          <textarea
            value={body}
            onChange={(e) => setBody(e.target.value)}
            placeholder="Description (optional)"
            rows={3}
            className="w-full resize-y rounded-md border border-border bg-surface px-2 py-1.5 text-sm outline-none focus-visible:ring-2 focus-visible:ring-ring/70"
          />
          <div className="flex justify-end gap-2">
            <Button variant="ghost" size="sm" onClick={() => setComposing(false)}>
              Cancel
            </Button>
            <Button size="sm" onClick={submitNew} disabled={!title.trim() || create.isPending}>
              Open issue
            </Button>
          </div>
          {create.error && (
            <p className="text-xs text-status-blocked">
              {create.error instanceof ApiError ? create.error.message : "Could not open issue."}
            </p>
          )}
        </div>
      )}

      {isLoading && <Skeleton className="h-24 w-full" />}

      {error && (
        <EmptyState
          icon={GitPullRequestClosed}
          title="Can't load issues"
          description={
            error instanceof ApiError ? error.message : "Unexpected error"
          }
        />
      )}

      {issues && issues.length === 0 && !isLoading && (
        <EmptyState
          icon={CircleDot}
          title={`No ${filter === "all" ? "" : filter} issues`}
          description="Nothing to show for this repo and filter."
        />
      )}

      <ul className="divide-y divide-border rounded-md border border-border">
        {issues?.map((issue) => {
          const open = issue.state.toUpperCase() === "OPEN";
          return (
            <li key={issue.number} className="flex items-start gap-3 px-3 py-2.5">
              {open ? (
                <CircleDot className="mt-0.5 size-4 shrink-0 text-status-running" />
              ) : (
                <CircleCheck className="mt-0.5 size-4 shrink-0 text-muted-foreground" />
              )}
              <div className="min-w-0 flex-1">
                <div className="flex items-center gap-2">
                  <span className="truncate text-sm font-medium">{issue.title}</span>
                  <span className="shrink-0 text-xs text-muted-foreground">
                    #{issue.number}
                  </span>
                </div>
                <div className="mt-1 flex flex-wrap items-center gap-1.5">
                  {issue.author && (
                    <span className="text-[11px] text-muted-foreground">
                      by {issue.author}
                    </span>
                  )}
                  {issue.labels.map((l) => (
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
                <a
                  href={issue.url}
                  target="_blank"
                  rel="noreferrer"
                  className="rounded p-1 text-muted-foreground hover:text-foreground"
                  title="Open on GitHub"
                >
                  <ExternalLink className="size-3.5" />
                </a>
                {open && (
                  <Button
                    variant="ghost"
                    size="sm"
                    disabled={close.isPending}
                    onClick={() => close.mutate({ dir, number: issue.number })}
                  >
                    Close
                  </Button>
                )}
              </div>
            </li>
          );
        })}
      </ul>
    </div>
  );
}
