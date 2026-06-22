import { useState } from "react";
import { CheckCircle2, CircleAlert, ServerCrash } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { EmptyState } from "@/components/ui/empty-state";
import { Skeleton } from "@/components/ui/skeleton";
import { cn } from "@/lib/utils/cn";
import { ApiError } from "@/lib/api/client";
import { relativeTime } from "@/lib/utils/platform";
import { useRunFollowUps, useResolveFollowUp } from "@/lib/hooks/use-follow-ups";
import type { FollowUp } from "@/types/follow-up";

/** The Follow-ups tab: the run's "needs a human" surface. The scheduler files a
 *  follow-up when it hits a wall it can't clear (an agent stuck on a consent
 *  screen, a spawn failure, a missing credential) instead of silently re-failing;
 *  agents file their own. Lazy-loaded (the query is gated on a non-null run) and
 *  polled, so a freshly-filed follow-up appears without a manual refresh. The
 *  operator resolves one once they've cleared the underlying cause. */
export function WorkflowFollowUps({ run }: { run: string | null }) {
  // Default to open-only — the actionable set — with a toggle to see resolved.
  const [showResolved, setShowResolved] = useState(false);
  const { data, isLoading, error } = useRunFollowUps(
    run,
    showResolved ? undefined : "open",
  );
  const resolve = useResolveFollowUp(run);

  if (!run) {
    return (
      <EmptyState
        icon={CheckCircle2}
        title="No follow-ups"
        description="Start the workflow — anything needing your attention shows up here."
      />
    );
  }

  if (error) {
    return (
      <EmptyState
        icon={ServerCrash}
        title="Can't load follow-ups"
        description={error instanceof ApiError ? error.message : "Unexpected error"}
      />
    );
  }

  const items = data ?? [];

  return (
    <div className="space-y-3">
      <div className="flex items-center justify-between gap-2">
        <p className="text-xs text-muted-foreground">
          {showResolved
            ? "All follow-ups for this workflow."
            : "Open follow-ups — things the orchestrator can't clear on its own."}
        </p>
        <Button
          size="sm"
          variant="ghost"
          onClick={() => setShowResolved((v) => !v)}
        >
          {showResolved ? "Hide resolved" : "Show resolved"}
        </Button>
      </div>

      {isLoading ? (
        <div className="space-y-2">
          {Array.from({ length: 3 }).map((_, i) => (
            <Skeleton key={i} className="h-20 w-full" />
          ))}
        </div>
      ) : items.length === 0 ? (
        <EmptyState
          icon={CheckCircle2}
          title={showResolved ? "Nothing here" : "All clear"}
          description={
            showResolved
              ? "No follow-ups have been filed for this workflow."
              : "No open follow-ups — nothing is waiting on you."
          }
        />
      ) : (
        items.map((f) => (
          <FollowUpCard
            key={f.id}
            item={f}
            onResolve={() => resolve.mutate(f.id)}
            resolving={resolve.isPending && resolve.variables === f.id}
          />
        ))
      )}
    </div>
  );
}

const KIND_LABEL: Record<string, string> = {
  consent: "consent",
  credential: "credential",
  spawn: "spawn",
  worktree: "worktree",
  gate: "gate",
  note: "note",
};

function FollowUpCard({
  item,
  onResolve,
  resolving,
}: {
  item: FollowUp;
  onResolve: () => void;
  resolving: boolean;
}) {
  const open = item.status === "open";
  return (
    <div
      className={cn(
        "rounded-lg border p-4",
        open
          ? "border-status-blocked/30 bg-status-blocked/5"
          : "border-border bg-surface opacity-80",
      )}
    >
      <div className="flex items-start justify-between gap-3">
        <div className="min-w-0">
          <div className="flex flex-wrap items-center gap-2">
            {open ? (
              <CircleAlert className="size-4 shrink-0 text-status-blocked" />
            ) : (
              <CheckCircle2 className="size-4 shrink-0 text-muted-foreground" />
            )}
            <span className="text-sm font-medium text-foreground">
              {item.title}
            </span>
            <Badge variant="outline" className="h-5 px-2 text-[10px]">
              {KIND_LABEL[item.kind] ?? item.kind}
            </Badge>
            {item.task && (
              <span className="font-mono text-[11px] text-muted-foreground">
                {item.task}
              </span>
            )}
            {item.seen > 1 && (
              <Badge variant="default" className="h-5 px-2 text-[10px]">
                hit {item.seen}×
              </Badge>
            )}
          </div>
        </div>
        {open && (
          <Button
            size="sm"
            variant="secondary"
            className="shrink-0"
            onClick={onResolve}
            disabled={resolving}
          >
            <CheckCircle2 />
            {resolving ? "Resolving…" : "Resolve"}
          </Button>
        )}
      </div>

      {/* The detail is markdown; render it as readable pre-wrapped text rather
          than pulling in a markdown renderer — it's short and operator-facing. */}
      <p className="mt-2 whitespace-pre-wrap break-words text-xs text-muted-foreground">
        {item.detail}
      </p>

      <p className="mt-2 text-[11px] text-muted-foreground/70">
        filed by {item.actor} · {relativeTime(item.updated_at)}
        {!open && item.resolved_at && ` · resolved ${relativeTime(item.resolved_at)}`}
      </p>
    </div>
  );
}
