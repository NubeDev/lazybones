import { request } from "./client";
import type { RetryStrategy, Task, WorktreeMode } from "@/types/task";
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

/** `GET /workflows/:id/tasks` — only the tasks linked to this workflow
 *  (filtered by `run_id` server-side); `404` if the workflow is absent. This is
 *  the hardened replacement for fetching every task and filtering in the browser
 *  — a foreign task can no longer reach a workflow view. */
export function listWorkflowTasks(id: string, signal?: AbortSignal): Promise<Task[]> {
  return request<Task[]>(`/workflows/${encodeURIComponent(id)}/tasks`, { signal });
}

/** The authored workspace block for creating a workflow. */
export interface WorkspaceDraft {
  repo: string;
  base_branch: string | null;
  branch_prefix: string | null;
  worktree_mode: WorktreeMode;
  /**
   * Names the shared worktree dir + branch (New/Shared modes), overriding the
   * id-derived default. null keeps today's behaviour. Two workflows with the
   * same name build in ONE tree — pick an existing worktree's name to attach to
   * it, or a fresh name to create one.
   */
  worktree_name?: string | null;
  /** Default agent triple for the workflow's tasks; null inherits the global. */
  tool: string | null;
  model: string | null;
  effort: string | null;
  /** Open a GitHub PR automatically once every task is done. null/false = off. */
  auto_pr?: boolean | null;
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

/** `PATCH /workflows/:id` — edit a workflow's workspace defaults (the inheritable
 *  git + agent config). `repo` is ignored server-side (kept as-is), so it's not in
 *  the draft. `404` if the workflow is absent. Returns the updated detail. */
export function updateWorkflow(
  id: string,
  workspace: WorkspaceDraft,
): Promise<WorkflowDetail> {
  return request<WorkflowDetail>(`/workflows/${encodeURIComponent(id)}`, {
    method: "PATCH",
    auth: true,
    // The server keeps the existing repo; send the current one to satisfy the
    // shared workspace body shape.
    body: { workspace },
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

/** `POST /workflows/:id/stop` — pause the workflow (lifecycle=stopped): kill live
 *  agents and reclaim running tasks back to ready (work kept), and promote/claim
 *  nothing until resumed. Fully reversible — NOT a terminal tombstone. */
export function stopWorkflow(id: string): Promise<WorkflowSummary> {
  return request<WorkflowSummary>(
    `/workflows/${encodeURIComponent(id)}/stop`,
    { method: "POST", auth: true },
  );
}

/** `POST /workflows/:id/stop-reset` — pause the workflow AND reset its unfinished
 *  tasks to pending (throw in-flight progress away), keeping done tasks. Still
 *  resumable — resume re-promotes from the reset state. NOT terminal. */
export function stopResetWorkflow(id: string): Promise<WorkflowSummary> {
  return request<WorkflowSummary>(
    `/workflows/${encodeURIComponent(id)}/stop-reset`,
    { method: "POST", auth: true },
  );
}

/** Options for restarting a workflow. The default (empty body) is a **hard
 *  reset**: re-run everything, remove worktrees, delete the workflow's branch(es)
 *  locally and on the remote. */
export interface RestartOptions {
  /** Soften to a resume-style restart: keep done tasks (reset only the unfinished
   *  part) and keep each task's worktree + branch. Default `false` (hard reset). */
  soft?: boolean;
}

/** `POST /workflows/:id/restart` — reset the workflow's tasks to pending so it
 *  can run from the beginning. Kills live agents; does not auto-start. */
export function restartWorkflow(
  id: string,
  opts: RestartOptions = {},
): Promise<WorkflowSummary> {
  return request<WorkflowSummary>(
    `/workflows/${encodeURIComponent(id)}/restart`,
    { method: "POST", auth: true, body: opts },
  );
}

/** `POST /workflows/:id/resume` — un-pause the workflow: flip lifecycle → active
 *  and reset only its blocked tasks to pending (continue from where it broke),
 *  leaving done/running/pending alone. The scheduler picks up on the next tick. */
export function resumeWorkflow(id: string): Promise<WorkflowSummary> {
  return request<WorkflowSummary>(
    `/workflows/${encodeURIComponent(id)}/resume`,
    { method: "POST", auth: true },
  );
}

/** `POST /tasks/:id/retry` — revive ONE blocked task. With a `strategy`, the task
 *  is revived in its kept worktree with that fix-intent guidance folded into the
 *  re-spawn prompt (it builds on its partial work). Without, it is reset clean to
 *  pending (the transient case; `remove_worktrees` also tears down its tree).
 *  `409` if the task isn't blocked, `404` if unknown. Returns the updated task. */
export function retryTask(
  id: string,
  opts: { strategy?: RetryStrategy; remove_worktrees?: boolean } = {},
): Promise<Task> {
  return request<Task>(`/tasks/${encodeURIComponent(id)}/retry`, {
    method: "POST",
    auth: true,
    body: opts,
  });
}

/** `PUT /tasks/:id/auto-retry` — set or clear a task's hands-off retry policy.
 *  `strategy: null` turns auto-retry off; omitting `max_retries` leaves the cap
 *  unchanged. `404` if unknown. Returns the updated task. */
export function setAutoRetry(
  id: string,
  opts: { strategy: RetryStrategy | null; max_retries?: number },
): Promise<Task> {
  return request<Task>(`/tasks/${encodeURIComponent(id)}/auto-retry`, {
    method: "PUT",
    auth: true,
    body: opts,
  });
}

/** `DELETE /workflows/:id` — hard-delete the workflow + its tasks. `409` if it
 *  still has live tasks (stop first). Returns whether it existed. */
export function deleteWorkflow(id: string): Promise<{ deleted: boolean }> {
  return request<{ deleted: boolean }>(
    `/workflows/${encodeURIComponent(id)}`,
    { method: "DELETE", auth: true },
  );
}

export type { Workspace };
