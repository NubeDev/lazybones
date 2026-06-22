import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  deleteWorkflowManagementAgent,
  getManagementAgent,
  getWorkflowManagementAgent,
  updateManagementAgent,
  updateWorkflowManagementAgent,
} from "@/lib/api/management-agent";
import type { ManagementAgentDraft } from "@/types/management-agent";

/** Read the single Lazybones-Agent configuration. */
export function useManagementAgentConfig() {
  return useQuery({
    queryKey: ["management-agent"],
    queryFn: ({ signal }) => getManagementAgent(signal),
    retry: false,
  });
}

/** Save the global Lazybones-Agent configuration (`PUT /settings/management-agent`). */
export function useUpdateManagementAgentConfig() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (draft: ManagementAgentDraft) => updateManagementAgent(draft),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["management-agent"] }),
  });
}

/** The *resolved* config for a workflow (its override if set, else global). */
export function useWorkflowManagementAgentConfig(workflowId: string | null) {
  return useQuery({
    queryKey: ["management-agent", "workflow", workflowId],
    queryFn: ({ signal }) => getWorkflowManagementAgent(workflowId as string, signal),
    enabled: workflowId != null,
    retry: false,
  });
}

/** Set a per-workflow config override. */
export function useUpdateWorkflowManagementAgentConfig(workflowId: string) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (draft: ManagementAgentDraft) =>
      updateWorkflowManagementAgent(workflowId, draft),
    onSuccess: () =>
      qc.invalidateQueries({ queryKey: ["management-agent", "workflow", workflowId] }),
  });
}

/** Drop a per-workflow override, reverting to the global default. */
export function useDeleteWorkflowManagementAgentConfig(workflowId: string) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: () => deleteWorkflowManagementAgent(workflowId),
    onSuccess: () =>
      qc.invalidateQueries({ queryKey: ["management-agent", "workflow", workflowId] }),
  });
}
