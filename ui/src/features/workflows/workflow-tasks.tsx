import { GitCommitHorizontal, ListChecks } from "lucide-react";
import { StatusBadge } from "@/components/ui/status-badge";
import { EmptyState } from "@/components/ui/empty-state";
import { Separator } from "@/components/ui/separator";
import { WORKTREE_MODES } from "@/features/tasks/worktree-mode";
import { FieldRow, Mono } from "@/features/tasks/detail/field-row";
import { SpecView } from "@/features/tasks/detail/spec-view";
import { TaskRetryControls } from "@/features/tasks/detail/task-retry-controls";
import { duration, relativeTime, shortTime } from "@/lib/utils/platform";
import type { Task } from "@/types/task";

/** The Tasks tab: every task in the workflow with its settings inlined, so the
 *  user can review specs, deps, worktree modes and provisioning without opening
 *  each task one at a time. Same fields as the task detail inspector. */
export function WorkflowTasks({ tasks }: { tasks: Task[] }) {
  if (tasks.length === 0) {
    return (
      <EmptyState
        icon={ListChecks}
        title="No tasks yet"
        description="Add tasks to this workflow — their settings show up here."
      />
    );
  }

  // Stable order: dependents after their deps where possible, else by id.
  const ordered = [...tasks].sort((a, b) => a.id.localeCompare(b.id));

  return (
    <div className="space-y-3">
      {ordered.map((task) => (
        <TaskSettingsCard key={task.id} task={task} />
      ))}
    </div>
  );
}

function TaskSettingsCard({ task }: { task: Task }) {
  const isBlocked = task.status === "blocked";

  return (
    <div className="rounded-lg border border-border bg-surface p-4">
      <div className="flex items-start justify-between gap-3">
        <div className="min-w-0">
          <div className="flex items-center gap-2">
            <span className="font-mono text-sm font-bold">{task.id}</span>
            <StatusBadge status={task.status} />
          </div>
          <p className="mt-1 text-sm text-muted-foreground">{task.title}</p>
        </div>
      </div>

      <div className="mt-3 grid gap-4 md:grid-cols-2">
        <div>
          <SectionLabel>Spec</SectionLabel>
          <SpecView spec={task.spec} />
        </div>

        <div>
          <SectionLabel>Settings</SectionLabel>
          <div className="divide-y divide-border">
            <FieldRow label="Run">{task.run}</FieldRow>
            <FieldRow label="Tool">{task.tool ?? "config default"}</FieldRow>
            <FieldRow label="Worktree mode">
              {WORKTREE_MODES[task.worktree_mode_override ?? task.worktree_mode].label}
              {task.worktree_mode_override && (
                <span className="ml-1 text-[10px] text-muted-foreground">(override)</span>
              )}
            </FieldRow>
            {task.deps.length > 0 && (
              <FieldRow label="Dependencies">
                <Mono>{task.deps.join(", ")}</Mono>
              </FieldRow>
            )}
            {task.owns.length > 0 && (
              <FieldRow label="Owns">
                <div className="flex flex-col items-end gap-0.5">
                  {task.owns.map((g) => (
                    <Mono key={g}>{g}</Mono>
                  ))}
                </div>
              </FieldRow>
            )}
            {task.reuse_from && (
              <FieldRow label="Reuses worktree of"><Mono>{task.reuse_from}</Mono></FieldRow>
            )}
            {task.session && <FieldRow label="Session"><Mono>{task.session}</Mono></FieldRow>}
            {task.worktree && <FieldRow label="Worktree"><Mono>{task.worktree}</Mono></FieldRow>}
            {task.branch && <FieldRow label="Branch"><Mono>{task.branch}</Mono></FieldRow>}
            {task.commit && (
              <FieldRow label="Commit">
                <span className="inline-flex items-center gap-1">
                  <GitCommitHorizontal className="size-3 text-muted-foreground" />
                  <Mono>{task.commit.slice(0, 12)}</Mono>
                </span>
              </FieldRow>
            )}
            {task.heartbeat && (
              <FieldRow label="Heartbeat">{relativeTime(task.heartbeat)}</FieldRow>
            )}
            <TaskTimingRows task={task} />
          </div>
        </div>
      </div>

      {isBlocked && task.reason && (
        <>
          <Separator className="my-3" />
          <div className="rounded-md border border-status-blocked/30 bg-status-blocked/10 p-3">
            <p className="text-[11px] font-medium uppercase tracking-wide text-status-blocked">
              Blocked
            </p>
            <p className="mt-1 text-xs text-foreground">{task.reason}</p>
          </div>
        </>
      )}

      {/* Retry-on-fail: strategy buttons (when blocked) + the auto-retry policy.
          Shared with the task inspector so both surfaces stay in lockstep. */}
      <Separator className="my-3" />
      <TaskRetryControls task={task} />
    </div>
  );
}

/** The lifecycle timing block: when the task started, finished or failed, and
 *  how long it took. Reused by the task inspector so both surfaces match. Rows
 *  only render when the underlying stamp exists, so an unstarted task shows
 *  nothing. "Took" runs start→finish (done), start→fail (blocked), or
 *  start→now while the task is still live. */
export function TaskTimingRows({ task }: { task: Task }) {
  if (!task.started_at && !task.finished_at && !task.failed_at) return null;

  // The instant the task settled, if it has — done wins over a (cleared) fail.
  const settled = task.finished_at ?? task.failed_at ?? null;
  const live = !settled && !!task.started_at;

  return (
    <>
      {task.started_at && (
        <FieldRow label="Started">{shortTime(task.started_at)}</FieldRow>
      )}
      {task.finished_at && (
        <FieldRow label="Finished">{shortTime(task.finished_at)}</FieldRow>
      )}
      {task.failed_at && (
        <FieldRow label="Failed">
          <span className="text-status-blocked">{shortTime(task.failed_at)}</span>
        </FieldRow>
      )}
      {task.started_at && (
        <FieldRow label={live ? "Running for" : "Took"}>
          {duration(task.started_at, settled)}
        </FieldRow>
      )}
    </>
  );
}

function SectionLabel({ children }: { children: React.ReactNode }) {
  return (
    <h4 className="mb-2 text-[11px] font-semibold uppercase tracking-wider text-muted-foreground">
      {children}
    </h4>
  );
}
