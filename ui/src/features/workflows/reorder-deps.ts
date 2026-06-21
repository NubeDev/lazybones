import type { Task } from "@/types/task";
import { layerTasks } from "./plan-graph";

/** The execution order a workflow's tasks run in, derived from the dependency
 *  graph: `layerTasks` assigns each task a depth (longest path from a root), and
 *  we flatten the layers — tasks in the same layer have no ordering between them
 *  (they run in parallel), so we break ties by id for a stable, repeatable list.
 *
 *  This is the same order the Tasks tab renders, and the order the reorder dialog
 *  starts from. `level` is the dependency depth (0-based); the UI shows it +1 as
 *  the human "step". */
export interface OrderedTask {
  task: Task;
  /** Dependency depth (0 = no in-workflow deps). Tasks sharing a level run in
   *  parallel; the badge shows `level + 1`. */
  level: number;
}

/** Flatten the dependency layers into the workflow's execution order. */
export function executionOrder(tasks: Task[]): OrderedTask[] {
  return layerTasks(tasks)
    .flatMap((layer) =>
      [...layer].sort((a, b) => a.task.id.localeCompare(b.task.id)),
    )
    .map((node) => ({ task: node.task, level: node.level }));
}

/** The new dependency set for a task after a drag-reorder that linearises the
 *  workflow: in the reordered list, each task should run after the one directly
 *  above it. We compute that as "depend on the immediate predecessor", but keep
 *  the task's *existing* deps that still point at an earlier task in the new order
 *  (so a real, intended edge the user already had isn't silently dropped) and
 *  discard any dep that would now point forward (which would be a cycle).
 *
 *  `order` is the task ids top-to-bottom as the user arranged them. Returns a map
 *  of taskId → new deps, only for tasks whose deps actually change. */
export function linearisedDeps(
  order: string[],
  byId: Map<string, Task>,
): Map<string, string[]> {
  const position = new Map(order.map((id, i) => [id, i]));
  const changed = new Map<string, string[]>();

  order.forEach((id, i) => {
    const task = byId.get(id);
    if (!task) return;

    // Keep only existing in-workflow deps that are still upstream in the new
    // order — a dep on a task now placed *below* this one would be a cycle.
    const kept = task.deps.filter((d) => {
      const p = position.get(d);
      return p !== undefined && p < i;
    });

    const next = new Set(kept);
    // Chain edge: run after the task directly above (the first row has none).
    if (i > 0) next.add(order[i - 1]);

    const nextArr = [...next];
    if (!sameSet(task.deps, nextArr)) changed.set(id, nextArr);
  });

  return changed;
}

/** Order-insensitive equality for two id lists. */
function sameSet(a: string[], b: string[]): boolean {
  if (a.length !== b.length) return false;
  const s = new Set(a);
  return b.every((x) => s.has(x));
}
