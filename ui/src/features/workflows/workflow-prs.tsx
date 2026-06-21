import { useState } from "react";
import {
  GitPullRequest,
  GitPullRequestClosed,
  GitMerge,
  ExternalLink,
  ChevronDown,
  AlertTriangle,
} from "lucide-react";
import { Button } from "@/components/ui/button";
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
  useMergeGhPr,
  useCloseGhPr,
} from "@/lib/hooks/use-gh";
import type { GhPullRequest, MergeMethod, PrStateFilter } from "@/types/gh";

const FILTERS: PrStateFilter[] = ["open", "closed", "merged", "all"];

const MERGE_METHODS: { method: MergeMethod; label: string }[] = [
  { method: "merge", label: "Create a merge commit" },
  { method: "squash", label: "Squash and merge" },
  { method: "rebase", label: "Rebase and merge" },
];

/** GitHub pull requests for the workflow's repo, via the user's existing `gh`
 *  login. Lists/filters by state and lets the user merge (merge/squash/rebase)
 *  or close open PRs. `dir` is the workflow's repo path. */
export function WorkflowPrs({ dir }: { dir: string }) {
  const [filter, setFilter] = useState<PrStateFilter>("open");
  const auth = useGhAuth();
  const { data: prs, isLoading, error } = useGhPrs(dir, filter);
  const merge = useMergeGhPr();
  const close = useCloseGhPr();

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
      </div>

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
            pr={pr}
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

function PrRow({
  pr,
  mergePending,
  closePending,
  onMerge,
  onClose,
}: {
  pr: GhPullRequest;
  mergePending: boolean;
  closePending: boolean;
  onMerge: (method: MergeMethod) => void;
  onClose: () => void;
}) {
  const state = pr.state.toUpperCase();
  const open = state === "OPEN";
  const merged = state === "MERGED";
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
    <li className="flex items-start gap-3 px-3 py-2.5">
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
    </li>
  );
}
