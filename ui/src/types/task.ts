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

/** Mirror of `lazybones_store::RetryStrategy` — the fix intent for a revived
 *  (re-attempted) task, folded into its re-spawn prompt as guidance. Drives both
 *  a manual strategy-retry and the hands-off auto-retry loop. */
export type RetryStrategy = "long_term" | "quick";

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
  /** Per-task model id forwarded to the agent CLI; `null` inherits the
   *  run/global default (resolved most-specific-wins at execution time). */
  model: string | null;
  /** Per-task effort level forwarded to the agent CLI; `null` inherits the
   *  run/global default. */
  effort: string | null;
  worktree_mode: WorktreeMode;
  session: string | null;
  worktree: string | null;
  branch: string | null;
  commit: string | null;
  reason: string | null;
  heartbeat: string | null;
  /** RFC3339 — when the task first moved to `running` (kept across reclaims). */
  started_at: string | null;
  /** RFC3339 — when the task reached `done`. */
  finished_at: string | null;
  /** RFC3339 — the most recent `blocked` (failure); cleared on revive/restart. */
  failed_at: string | null;
  /** FK to the parent workflow run; `null` for a standalone task. This — not the
   *  `run` label — is the real relationship workflow views key off. */
  run_id: string | null;
  /** Provenance: which template this task was instantiated from, if any. */
  template_id: string | null;
  /** For `reuse` mode: the task id whose worktree to reuse. */
  reuse_from: string | null;
  /** Workflow-only override of the inherited worktree mode; `null` = inherit. */
  worktree_mode_override: WorktreeMode | null;
  /** Hands-off auto-retry strategy; `null` = off (a block waits for a human). */
  auto_retry: RetryStrategy | null;
  /** Cap on hands-off auto-retries before the task stays blocked (default 2). */
  max_retries: number;
  /** How many auto-retries have been spent (reset on a clean retry / done). */
  retry_count: number;
}
