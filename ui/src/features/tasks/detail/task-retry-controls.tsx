import { RotateCw, Wrench, Zap, RefreshCw } from "lucide-react";
import { Button } from "@/components/ui/button";
import { ApiError } from "@/lib/api/client";
import { useRetryTask, useSetAutoRetry } from "@/lib/hooks/use-workflows";
import type { RetryStrategy, Task } from "@/types/task";

/** Clamp the auto-retry cap to a sane inline range (at least 1 attempt). */
const MIN_RETRIES = 1;
const MAX_RETRIES = 10;

/** The retry-on-fail surface for a single task, shared by the workflow Tasks tab
 *  and the task inspector. Two parts:
 *
 *  - **Retry** (only when the task is `blocked`): pick how the re-attempt should
 *    approach the fix — `Fix long-term` / `Quick fix` revive the task in its kept
 *    worktree with that guidance folded into the re-spawn prompt; `Re-run clean`
 *    resets it to a fresh worktree for a transient/flaky failure.
 *  - **Auto-retry**: the hands-off policy — let the scheduler re-attempt future
 *    blocks of this task with a chosen strategy, up to a cap, before it waits for
 *    a human. Always shown so it can be set up *before* a failure. */
export function TaskRetryControls({ task }: { task: Task }) {
  const retry = useRetryTask();
  const isBlocked = task.status === "blocked";
  const retryErr = retry.error instanceof ApiError ? retry.error.message : null;

  const doRetry = (strategy?: RetryStrategy) =>
    retry.mutate({ id: task.id, strategy });

  return (
    <div className="space-y-3">
      {isBlocked && (
        <div>
          <p className="text-[11px] font-medium text-muted-foreground">
            Retry with a fix strategy
          </p>
          <div className="mt-1.5 flex flex-wrap gap-2">
            <Button
              size="sm"
              variant="outline"
              disabled={retry.isPending}
              title="Revive in place and fix the root cause properly, even if it takes more work"
              onClick={() => doRetry("long_term")}
            >
              <Wrench /> Fix long-term
            </Button>
            <Button
              size="sm"
              variant="outline"
              disabled={retry.isPending}
              title="Revive in place and apply the smallest change that gets it green"
              onClick={() => doRetry("quick")}
            >
              <Zap /> Quick fix
            </Button>
            <Button
              size="sm"
              variant="ghost"
              disabled={retry.isPending}
              title="Reset to pending with a fresh worktree — for a transient/flaky failure"
              onClick={() => doRetry()}
            >
              <RotateCw /> Re-run clean
            </Button>
          </div>
          {retryErr && (
            <p className="mt-2 text-[11px] text-status-blocked">{retryErr}</p>
          )}
        </div>
      )}

      <AutoRetryRow task={task} />
    </div>
  );
}

/** The hands-off auto-retry policy control: three exclusive choices (off /
 *  long-term / quick) plus an inline cap stepper. When on, the scheduler
 *  re-attempts a block of this task with the chosen strategy up to `max_retries`
 *  times before leaving it for a human. */
function AutoRetryRow({ task }: { task: Task }) {
  const mutation = useSetAutoRetry();
  const current = task.auto_retry; // null | "long_term" | "quick"
  const choices: { value: RetryStrategy | null; label: string }[] = [
    { value: null, label: "Off" },
    { value: "long_term", label: "Long-term" },
    { value: "quick", label: "Quick" },
  ];
  const err = mutation.error instanceof ApiError ? mutation.error.message : null;

  // Re-send the active strategy with a new cap (the server leaves the strategy
  // untouched, but passing it keeps the call self-describing and order-safe).
  const setCap = (next: number) => {
    const clamped = Math.max(MIN_RETRIES, Math.min(MAX_RETRIES, next));
    if (clamped === task.max_retries) return;
    mutation.mutate({ id: task.id, strategy: current, max_retries: clamped });
  };

  return (
    <div className="flex flex-wrap items-center gap-x-3 gap-y-1.5">
      <span className="inline-flex items-center gap-1 text-[11px] font-medium text-muted-foreground">
        <RefreshCw className="size-3" /> Auto-retry on block
      </span>
      <div className="inline-flex overflow-hidden rounded-md border border-border">
        {choices.map((c) => {
          const active = current === c.value;
          return (
            <button
              key={c.label}
              type="button"
              disabled={mutation.isPending}
              aria-pressed={active}
              className={`px-2 py-0.5 text-[11px] transition-colors disabled:opacity-50 ${
                active
                  ? "bg-primary text-primary-foreground"
                  : "bg-surface text-muted-foreground hover:bg-muted"
              }`}
              onClick={() => mutation.mutate({ id: task.id, strategy: c.value })}
            >
              {c.label}
            </button>
          );
        })}
      </div>
      {current && (
        <div className="inline-flex items-center gap-1.5 text-[11px] text-muted-foreground">
          <span>max</span>
          <div className="inline-flex items-center overflow-hidden rounded-md border border-border">
            <button
              type="button"
              aria-label="Decrease retry cap"
              disabled={mutation.isPending || task.max_retries <= MIN_RETRIES}
              className="px-1.5 py-0.5 text-foreground hover:bg-muted disabled:opacity-40"
              onClick={() => setCap(task.max_retries - 1)}
            >
              −
            </button>
            <span className="min-w-5 px-1 text-center font-mono text-foreground">
              {task.max_retries}
            </span>
            <button
              type="button"
              aria-label="Increase retry cap"
              disabled={mutation.isPending || task.max_retries >= MAX_RETRIES}
              className="px-1.5 py-0.5 text-foreground hover:bg-muted disabled:opacity-40"
              onClick={() => setCap(task.max_retries + 1)}
            >
              +
            </button>
          </div>
          <span>× · {task.retry_count} spent</span>
        </div>
      )}
      {err && <span className="text-[11px] text-status-blocked">{err}</span>}
    </div>
  );
}
