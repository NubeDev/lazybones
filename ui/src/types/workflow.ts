import type { WorktreeMode } from "./task";

/** Mirror of `lazybones_store::Template` — a reusable, stateless task recipe. */
export interface Template {
  id: string;
  title: string;
  description: string;
  spec_template: string;
  default_tool: string | null;
  default_model: string | null;
  default_effort: string | null;
  default_worktree_mode: WorktreeMode | null;
  created_at: string;
  updated_at: string;
}

/** Mirror of `lazybones_store::Workspace` — a workflow's repo + inherited git config. */
export interface Workspace {
  repo: string;
  base_branch: string | null;
  branch_prefix: string | null;
  worktree_mode: WorktreeMode;
  /** Default agent triple for the workflow's tasks; null inherits the global. */
  tool: string | null;
  model: string | null;
  effort: string | null;
  /** Open a GitHub PR automatically once every task is done (the engine spawns the
   *  configured agent to summarize, then `gh pr create`). null/false = off. */
  auto_pr?: boolean | null;
}

/** Human-set lifecycle (the only stored workflow state). `stopped` is a
 *  reversible pause — only `done` (derived) and a hard delete are terminal. */
export type Lifecycle = "active" | "stopped";

/** The derived, server-computed workflow state. Never compute this client-side. */
export type WorkflowState =
  | "draft"
  | "ready"
  | "running"
  | "needs-attention"
  | "done"
  | "stopped";

export const WORKFLOW_STATES: WorkflowState[] = [
  "draft",
  "ready",
  "running",
  "needs-attention",
  "done",
  "stopped",
];

/** Mirror of `lazybones_store::Run` — one concrete, one-off workflow. */
export interface Run {
  id: string;
  title: string;
  workspace: Workspace;
  lifecycle: Lifecycle;
  created_at: string;
  started_at: string | null;
  /** URL of the PR the engine auto-opened on completion; null until it has. */
  pr_url?: string | null;
}

/** `GET /workflows` item — the run plus its derived state and task counts. */
export interface WorkflowSummary extends Run {
  state: WorkflowState;
  task_count: number;
  done_count: number;
  /** RFC3339 — when the workflow reached a terminal state (latest terminal task
   *  stamp); `null` while still in flight. Derived server-side, never stored. */
  finished_at: string | null;
  /** RFC3339 — the latest task failure across the workflow, or `null`. */
  failed_at: string | null;
}

/** `GET /workflows/:id` detail — the summary plus the linked task ids. */
export interface WorkflowDetail extends WorkflowSummary {
  task_ids: string[];
}
