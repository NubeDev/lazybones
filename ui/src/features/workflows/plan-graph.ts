import type { Task } from "@/types/task";

/** A task placed into a dependency layer for the plan graph. */
export interface GraphNode {
  task: Task;
  /** Longest-path depth from a root (0 = root). Drives the column. */
  level: number;
}

/** Layer a set of tasks by dependency depth so the plan graph draws the real
 *  shape (fan-out included), not just lifecycle columns. Only deps that are
 *  themselves in this set count, so a workflow's graph is self-contained.
 *
 *  Cycles (shouldn't happen — the backend builds a DAG) are broken defensively
 *  by capping the recursion at the node count. */
export function layerTasks(tasks: Task[]): GraphNode[][] {
  const ids = new Set(tasks.map((t) => t.id));
  const byId = new Map(tasks.map((t) => [t.id, t]));
  const memo = new Map<string, number>();

  function depth(id: string, seen: Set<string>): number {
    const cached = memo.get(id);
    if (cached !== undefined) return cached;
    if (seen.has(id)) return 0; // cycle guard
    const task = byId.get(id);
    if (!task) return 0;
    const inSet = task.deps.filter((d) => ids.has(d));
    if (inSet.length === 0) {
      memo.set(id, 0);
      return 0;
    }
    const next = new Set(seen).add(id);
    const d = 1 + Math.max(...inSet.map((dep) => depth(dep, next)));
    memo.set(id, d);
    return d;
  }

  const nodes: GraphNode[] = tasks.map((t) => ({
    task: t,
    level: depth(t.id, new Set()),
  }));

  const maxLevel = nodes.reduce((m, n) => Math.max(m, n.level), 0);
  const layers: GraphNode[][] = Array.from({ length: maxLevel + 1 }, () => []);
  for (const n of nodes) layers[n.level].push(n);
  return layers;
}
