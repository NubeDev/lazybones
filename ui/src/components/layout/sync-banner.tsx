import { useState } from "react";
import { AlertCircle, RefreshCw, X } from "lucide-react";
import { Button } from "@/components/ui/button";
import {
  useSyncStatus,
  usePullSync,
  isBehind,
} from "@/lib/hooks/use-content-sync";

/** A floating banner that appears when the local store is behind the sync remote
 *  ("hey, you're out of sync — do you wanna pull?"). Dismissible for the session;
 *  reappears on the next status poll if still behind. Renders nothing in every
 *  other state, so it's safe to mount globally. */
export function SyncBanner() {
  const { data } = useSyncStatus();
  const pull = usePullSync();
  const [dismissed, setDismissed] = useState(false);

  if (dismissed || !isBehind(data)) return null;

  const diverged = data?.state === "diverged";
  const behind = data?.behind ?? 0;

  return (
    <div className="pointer-events-auto fixed left-1/2 top-3 z-50 w-full max-w-md -translate-x-1/2 rounded-lg border border-status-blocked/40 bg-surface p-3 shadow-lg">
      <div className="flex items-start gap-3">
        <AlertCircle className="mt-0.5 size-4 shrink-0 text-status-blocked" />
        <div className="min-w-0 flex-1">
          <p className="text-sm font-medium">
            {diverged ? "Sync repo has diverged" : "You're out of sync"}
          </p>
          <p className="text-xs text-muted-foreground">
            {diverged
              ? "Local and remote both have changes — pull may need a manual reconcile."
              : `The sync repo has ${behind} change${behind === 1 ? "" : "s"} you don't have yet. Pull them in?`}
          </p>
        </div>
        <div className="flex shrink-0 items-center gap-1.5">
          <Button
            size="sm"
            onClick={() => pull.mutate(undefined, { onSuccess: () => setDismissed(true) })}
            disabled={pull.isPending}
          >
            <RefreshCw className="size-3.5" /> {pull.isPending ? "Pulling…" : "Pull"}
          </Button>
          <button
            type="button"
            aria-label="Dismiss"
            onClick={() => setDismissed(true)}
            className="rounded p-1 text-muted-foreground transition-colors hover:bg-muted"
          >
            <X className="size-3.5" />
          </button>
        </div>
      </div>
    </div>
  );
}
