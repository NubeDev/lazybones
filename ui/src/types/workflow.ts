import type { WorktreeMode } from "./task";

/** Mirror of `lazybones_store::Template` — a reusable, stateless task recipe. */
export interface Template {
  id: string;
  title: string;
  description: string;
  spec_template: string;
  default_tool: string | null;
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
}

/** Human-set lifecycle (the only stored workflow state). */
export type Lifecycle = "active" | "cancelled";

/** The derived, server-computed workflow state. Never compute this client-side. */
export type WorkflowState =
  | "draft"
  | "ready"
  | "running"
  | "needs-attention"
  | "done"
  | "cancelled";

export const WORKFLOW_STATES: WorkflowState[] = [
  "draft",
  "ready",
  "running",
  "needs-attention",
  "done",
  "cancelled",
];

/** Mirror of `lazybones_store::Run` — one concrete, one-off workflow. */
export interface Run {
  id: string;
  title: string;
  workspace: Workspace;
  lifecycle: Lifecycle;
  created_at: string;
  started_at: string | null;
}

/** `GET /workflows` item — the run plus its derived state and task counts. */
export interface WorkflowSummary extends Run {
  state: WorkflowState;
  task_count: number;
  done_count: number;
}

/** `GET /workflows/:id` detail — the summary plus the linked task ids. */
export interface WorkflowDetail extends WorkflowSummary {
  task_ids: string[];
}
