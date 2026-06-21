import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  addWorkflowTask,
  cancelWorkflow,
  createWorkflow,
  deleteWorkflow,
  getWorkflow,
  listWorkflows,
  listWorkflowTasks,
  restartWorkflow,
  resumeWorkflow,
  retryTask,
  setAutoRetry,
  startWorkflow,
  type RestartOptions,
  type WorkflowTaskDraft,
  type WorkspaceDraft,
} from "@/lib/api/workflows";
import type { RetryStrategy } from "@/types/task";

/** Poll the workflow list (derived state + counts). */
export function useWorkflows() {
  return useQuery({
    queryKey: ["workflows"],
    queryFn: ({ signal }) => listWorkflows(signal),
    refetchInterval: 4000,
  });
}

/** One workflow's detail (workspace, derived state, task ids). */
export function useWorkflow(id: string | null) {
  return useQuery({
    queryKey: ["workflow", id],
    queryFn: ({ signal }) => getWorkflow(id!, signal),
    enabled: !!id,
    refetchInterval: 4000,
  });
}

/** A workflow's tasks, fetched server-side-filtered by `run_id` — the UI never
 *  sees tasks from other workflows. Keyed under the workflow so it invalidates
 *  alongside the detail. */
export function useWorkflowTasks(id: string | null) {
  return useQuery({
    queryKey: ["workflow", id, "tasks"],
    queryFn: ({ signal }) => listWorkflowTasks(id!, signal),
    enabled: !!id,
    refetchInterval: 4000,
  });
}

/** Refresh both the list and any open detail after a workflow change. */
function invalidate(qc: ReturnType<typeof useQueryClient>) {
  qc.invalidateQueries({ queryKey: ["workflows"] });
  qc.invalidateQueries({ queryKey: ["workflow"] });
  qc.invalidateQueries({ queryKey: ["tasks"] });
}

/** Create a workflow (`POST /workflows`). */
export function useCreateWorkflow() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({
      id,
      title,
      workspace,
    }: {
      id: string;
      title: string;
      workspace: WorkspaceDraft;
    }) => createWorkflow(id, title, workspace),
    onSuccess: () => invalidate(qc),
  });
}

/** Add a task to a workflow (`POST /workflows/:id/tasks`). */
export function useAddWorkflowTask(workflowId: string) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (draft: WorkflowTaskDraft) => addWorkflowTask(workflowId, draft),
    onSuccess: () => invalidate(qc),
  });
}

/** Start a workflow → promote eligible roots (`POST /workflows/:id/start`). */
export function useStartWorkflow() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => startWorkflow(id),
    onSuccess: () => invalidate(qc),
  });
}

/** Cancel a workflow (`POST /workflows/:id/cancel`). */
export function useCancelWorkflow() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => cancelWorkflow(id),
    onSuccess: () => invalidate(qc),
  });
}

/** Restart a workflow → reset its tasks to pending (`POST /workflows/:id/restart`). */
export function useRestartWorkflow() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ id, opts }: { id: string; opts?: RestartOptions }) =>
      restartWorkflow(id, opts),
    onSuccess: () => invalidate(qc),
  });
}

/** Resume a workflow → reset only its blocked tasks (`POST /workflows/:id/resume`). */
export function useResumeWorkflow() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => resumeWorkflow(id),
    onSuccess: () => invalidate(qc),
  });
}

/** Retry one blocked task (`POST /tasks/:id/retry`). With a `strategy` the task is
 *  revived in place with guidance; without, it is reset clean to pending. */
export function useRetryTask() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ id, strategy }: { id: string; strategy?: RetryStrategy }) =>
      retryTask(id, strategy ? { strategy } : {}),
    onSuccess: () => invalidate(qc),
  });
}

/** Set/clear a task's hands-off auto-retry policy (`PUT /tasks/:id/auto-retry`). */
export function useSetAutoRetry() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({
      id,
      strategy,
      max_retries,
    }: {
      id: string;
      strategy: RetryStrategy | null;
      max_retries?: number;
    }) => setAutoRetry(id, { strategy, max_retries }),
    onSuccess: () => invalidate(qc),
  });
}

/** Delete a workflow + its tasks (`DELETE /workflows/:id`). */
export function useDeleteWorkflow() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => deleteWorkflow(id),
    onSuccess: () => invalidate(qc),
  });
}
