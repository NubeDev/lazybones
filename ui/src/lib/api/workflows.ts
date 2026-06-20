import { request } from "./client";
import type { Task, WorktreeMode } from "@/types/task";
import type {
  Run,
  Workspace,
  WorkflowDetail,
  WorkflowSummary,
} from "@/types/workflow";

/** `GET /workflows` — list workflows with derived state + task counts. */
export function listWorkflows(signal?: AbortSignal): Promise<WorkflowSummary[]> {
  return request<WorkflowSummary[]>("/workflows", { signal });
}

/** `GET /workflows/:id` — detail (summary + linked task ids); `404` if absent. */
export function getWorkflow(id: string, signal?: AbortSignal): Promise<WorkflowDetail> {
  return request<WorkflowDetail>(`/workflows/${encodeURIComponent(id)}`, { signal });
}

/** The authored workspace block for creating a workflow. */
export interface WorkspaceDraft {
  repo: string;
  base_branch: string | null;
  branch_prefix: string | null;
  worktree_mode: WorktreeMode;
  /** Default agent triple for the workflow's tasks; null inherits the global. */
  tool: string | null;
  model: string | null;
  effort: string | null;
}

/** `POST /workflows` — create a workflow bound to a workspace. `409` if id taken. */
export function createWorkflow(
  id: string,
  title: string,
  workspace: WorkspaceDraft,
): Promise<Run> {
  return request<Run>("/workflows", {
    method: "POST",
    auth: true,
    body: { id, title, workspace },
  });
}

/** A task to add to a workflow. `from_template` supplies the spec when set. */
export interface WorkflowTaskDraft {
  id: string;
  title: string;
  spec?: string;
  from_template?: string | null;
  deps?: string[];
  owns?: string[];
  tool?: string | null;
  model?: string | null;
  effort?: string | null;
  worktree_mode_override?: WorktreeMode | null;
  reuse_from?: string | null;
}

/** `POST /workflows/:id/tasks` — add a task. `404` unknown wf, `409` dup/missing template. */
export function addWorkflowTask(
  workflowId: string,
  draft: WorkflowTaskDraft,
): Promise<Task> {
  return request<Task>(`/workflows/${encodeURIComponent(workflowId)}/tasks`, {
    method: "POST",
    auth: true,
    body: draft,
  });
}

/** `POST /workflows/:id/start` — promote eligible root tasks → ready. */
export function startWorkflow(id: string): Promise<{ promoted: string[] }> {
  return request<{ promoted: string[] }>(
    `/workflows/${encodeURIComponent(id)}/start`,
    { method: "POST", auth: true },
  );
}

/** `POST /workflows/:id/cancel` — set lifecycle=cancelled; block + kill agents. */
export function cancelWorkflow(id: string): Promise<WorkflowSummary> {
  return request<WorkflowSummary>(
    `/workflows/${encodeURIComponent(id)}/cancel`,
    { method: "POST", auth: true },
  );
}

export type { Workspace };
