import type { Status, Task } from "@/types/task";
import { STATUSES } from "@/types/task";

/** The board columns, in lifecycle order. */
export const BOARD_COLUMNS: Status[] = STATUSES;

/** Bucket tasks by status, preserving column order and stable id sort within. */
export function groupByStatus(tasks: Task[]): Record<Status, Task[]> {
  const groups = Object.fromEntries(STATUSES.map((s) => [s, [] as Task[]])) as Record<
    Status,
    Task[]
  >;
  for (const t of tasks) groups[t.status].push(t);
  for (const s of STATUSES) groups[s].sort((a, b) => a.id.localeCompare(b.id));
  return groups;
}

/** A coarse progress fraction (done / total) for a run's tasks. */
export function progress(tasks: Task[]): number {
  if (tasks.length === 0) return 0;
  const done = tasks.filter((t) => t.status === "done").length;
  return done / tasks.length;
}
