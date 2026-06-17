/** Mirror of `lazybones_store::Status` — the task lifecycle (lowercase wire form). */
export type Status =
  | "pending"
  | "ready"
  | "running"
  | "gating"
  | "done"
  | "blocked";

/** Mirror of `lazybones_store::WorktreeMode` — how the loop provisions the
 *  working tree when it claims this task. `new` = isolated worktree (default);
 *  `reuse` = an existing worktree path; `branch` = the main checkout on a branch. */
export type WorktreeMode = "new" | "reuse" | "branch";

export const STATUSES: Status[] = [
  "pending",
  "ready",
  "running",
  "gating",
  "done",
  "blocked",
];

/** Mirror of `lazybones_store::Task` — the durable task document. */
export interface Task {
  id: string;
  run: string;
  title: string;
  spec: string;
  status: Status;
  deps: string[];
  owns: string[];
  tool: string | null;
  worktree_mode: WorktreeMode;
  session: string | null;
  worktree: string | null;
  branch: string | null;
  commit: string | null;
  reason: string | null;
  heartbeat: string | null;
}
