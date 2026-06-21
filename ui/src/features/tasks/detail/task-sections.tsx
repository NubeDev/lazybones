import type { ReactNode } from "react";
import { GitCommitHorizontal } from "lucide-react";
import { WORKTREE_MODES } from "@/features/tasks/worktree-mode";
import { FieldRow, Mono } from "./field-row";
import { DepsList } from "./deps-list";
import { SpecView } from "./spec-view";
import { TaskRetryControls } from "./task-retry-controls";
import { TaskChat } from "./task-chat";
import { TaskTimingRows } from "@/features/workflows/workflow-tasks";
import { relativeTime } from "@/lib/utils/platform";
import type { Task } from "@/types/task";

/** Reusable building blocks for the task detail surfaces (the inspector panel and
 *  any future full task page). Each section is a self-contained component so the
 *  long task view can be split across tabs/columns instead of one endless scroll. */

/** A small uppercase section heading. */
export function SectionLabel({ children }: { children: ReactNode }) {
  return (
    <h4 className="mb-2 text-[11px] font-semibold uppercase tracking-wider text-muted-foreground">
      {children}
    </h4>
  );
}

/** A titled card wrapping one section of task content. */
export function SectionCard({
  label,
  children,
  className,
}: {
  label: string;
  children: ReactNode;
  className?: string;
}) {
  return (
    <section className={`rounded-lg border border-border bg-surface p-4 ${className ?? ""}`}>
      <SectionLabel>{label}</SectionLabel>
      {children}
    </section>
  );
}

/** The task's spec / instructions. */
export function SpecSection({ spec, loading }: { spec: string; loading?: boolean }) {
  return (
    <SectionCard label="Spec">
      <SpecView spec={spec} loading={loading} />
    </SectionCard>
  );
}

/** Dependency chips, clickable to jump between sibling tasks. */
export function DependenciesSection({
  task,
  byId,
  onSelect,
}: {
  task: Task;
  byId: Map<string, Task>;
  onSelect: (id: string) => void;
}) {
  return (
    <SectionCard label="Dependencies">
      <DepsList deps={task.deps} byId={byId} onSelect={onSelect} />
    </SectionCard>
  );
}

/** The provisioning + lifecycle facts: run, tool, worktree, branch, commit,
 *  timing, ownership. The bulk of the old single-column scroll. */
export function DetailsSection({ task }: { task: Task }) {
  return (
    <SectionCard label="Details">
      <div className="divide-y divide-border">
        <FieldRow label="Run">{task.run}</FieldRow>
        <FieldRow label="Tool">{task.tool ?? "config default"}</FieldRow>
        {task.session && <FieldRow label="Session"><Mono>{task.session}</Mono></FieldRow>}
        <FieldRow label="Worktree mode">
          {WORKTREE_MODES[task.worktree_mode_override ?? task.worktree_mode].label}
          {task.worktree_mode_override && (
            <span className="ml-1 text-[10px] text-muted-foreground">(override)</span>
          )}
        </FieldRow>
        {task.reuse_from && (
          <FieldRow label="Reuses worktree of"><Mono>{task.reuse_from}</Mono></FieldRow>
        )}
        {task.worktree && (
          <FieldRow label="Worktree"><Mono>{task.worktree}</Mono></FieldRow>
        )}
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
        {task.owns.length > 0 && (
          <FieldRow label="Owns">
            <div className="flex flex-col items-end gap-0.5">
              {task.owns.map((g) => (
                <Mono key={g}>{g}</Mono>
              ))}
            </div>
          </FieldRow>
        )}
      </div>
    </SectionCard>
  );
}

/** The blocked-reason callout — only renders when the task carries a reason. */
export function BlockedReason({ task }: { task: Task }) {
  if (!task.reason) return null;
  return (
    <div className="rounded-md border border-status-blocked/30 bg-status-blocked/10 p-3">
      <p className="text-[11px] font-medium uppercase tracking-wide text-status-blocked">
        Blocked
      </p>
      <p className="mt-1 text-xs text-foreground">{task.reason}</p>
    </div>
  );
}

/** The retry-on-fail strategy + auto-retry policy controls. */
export function RetrySection({ task }: { task: Task }) {
  return (
    <SectionCard label="Retry on fail">
      <TaskRetryControls task={task} />
    </SectionCard>
  );
}

/** Free-text chat with the task's agent. */
export function ChatSection({ task }: { task: Task }) {
  return (
    <SectionCard label="Chat" className="flex min-h-0 flex-1 flex-col">
      <TaskChat task={task} />
    </SectionCard>
  );
}
