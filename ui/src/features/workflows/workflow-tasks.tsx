import { useState } from "react";
import { GitCommitHorizontal, ListChecks, MessagesSquare, Settings2 } from "lucide-react";
import { StatusBadge } from "@/components/ui/status-badge";
import { EmptyState } from "@/components/ui/empty-state";
import { Separator } from "@/components/ui/separator";
import { Tabs, TabsList, TabsTrigger, TabsContent } from "@/components/ui/tabs";
import { WORKTREE_MODES } from "@/features/tasks/worktree-mode";
import { FieldRow, Mono } from "@/features/tasks/detail/field-row";
import { SpecView } from "@/features/tasks/detail/spec-view";
import { TaskRetryControls } from "@/features/tasks/detail/task-retry-controls";
import { executionOrder } from "./reorder-deps";
import { TaskLogsPanel } from "./task-logs-panel";
import { Badge } from "@/components/ui/badge";
import { Tooltip } from "@/components/ui/tooltip";
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

  // Workflow order: walk the dependency graph depth-first so each task lists
  // after the tasks it depends on (e.g. unit → tests → review → PR), matching
  // the Plan tab's left→right layout. `level` is the dependency depth — tasks
  // sharing a level run in parallel; the badge surfaces it as the run step.
  const ordered = executionOrder(tasks);

  return (
    <div className="space-y-3">
      {ordered.map(({ task, level }, i) => (
        <TaskSettingsCard key={task.id} task={task} step={i + 1} level={level} />
      ))}
    </div>
  );
}

function TaskSettingsCard({
  task,
  step,
  level,
}: {
  task: Task;
  step: number;
  level: number;
}) {
  const isBlocked = task.status === "blocked";
  // Per-card view toggle. Starts on Settings; the Logs trace lazy-loads only
  // once the user switches to it (TaskLogsPanel's query is gated on `active`),
  // so the user never leaves the page to read an agent's logs.
  const [view, setView] = useState("settings");

  return (
    <div className="rounded-lg border border-border bg-surface p-4">
      <div className="flex items-start justify-between gap-3">
        <div className="flex min-w-0 items-start gap-2.5">
          <Tooltip
            label={`Run step ${step} · dependency level ${level + 1}. Tasks at the same level run in parallel.`}
            side="right"
          >
            <Badge variant="accent" className="mt-0.5 shrink-0 tabular-nums">
              {step}
            </Badge>
          </Tooltip>
          <div className="min-w-0">
            <div className="flex items-center gap-2">
              <span className="font-mono text-sm font-bold">{task.id}</span>
              <StatusBadge status={task.status} />
            </div>
            <p className="mt-1 text-sm text-muted-foreground">{task.title}</p>
          </div>
        </div>
      </div>

      <Tabs value={view} onValueChange={setView} className="mt-3">
        <TabsList>
          <TabsTrigger value="settings">
            <Settings2 className="size-3.5" /> Settings
          </TabsTrigger>
          <TabsTrigger value="logs">
            <MessagesSquare className="size-3.5" /> Logs
          </TabsTrigger>
        </TabsList>

        <TabsContent value="settings" className="mt-3">
          <TaskSettings task={task} isBlocked={isBlocked} />
        </TabsContent>

        <TabsContent value="logs" className="mt-3">
          <TaskLogsPanel taskId={task.id} active={view === "logs"} />
        </TabsContent>
      </Tabs>
    </div>
  );
}

function TaskSettings({ task, isBlocked }: { task: Task; isBlocked: boolean }) {
  return (
    <>
      <div className="grid gap-4 md:grid-cols-2">
        <div>
          <SectionLabel>Spec</SectionLabel>
          <SpecView spec={task.spec} />
        </div>

        <div>
          <SectionLabel>Settings</SectionLabel>
          <div className="divide-y divide-border">
            <FieldRow label="Run">{task.run}</FieldRow>
            <FieldRow label="Tool">{task.tool ?? "config default"}</FieldRow>
            <FieldRow label="Model">
              {task.model ?? <Inherited />}
            </FieldRow>
            <FieldRow label="Effort">
              {task.effort ?? <Inherited />}
            </FieldRow>
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
    </>
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

/** Shown for a model/effort the task doesn't pin — it inherits the workflow's
 *  (or global) default, resolved most-specific-wins at execution time. */
export function Inherited() {
  return <span className="text-muted-foreground/70">inherited</span>;
}

function SectionLabel({ children }: { children: React.ReactNode }) {
  return (
    <h4 className="mb-2 text-[11px] font-semibold uppercase tracking-wider text-muted-foreground">
      {children}
    </h4>
  );
}
