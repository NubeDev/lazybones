import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  addWorkflowTask,
  cancelWorkflow,
  createWorkflow,
  getWorkflow,
  listWorkflows,
  startWorkflow,
  type WorkflowTaskDraft,
  type WorkspaceDraft,
} from "@/lib/api/workflows";

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
