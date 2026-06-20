import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  createAgentCatalog,
  deleteAgentCatalog,
  listAgentCatalog,
  updateAgentCatalog,
  type AgentCatalogDraft,
} from "@/lib/api/agent-catalog";

/** The CRUD-able agent catalog — agents + their model/effort menus. Slow-
 *  changing config; cached and reused across the add-task and settings UIs. */
export function useAgentCatalog() {
  return useQuery({
    queryKey: ["agent-catalog"],
    queryFn: ({ signal }) => listAgentCatalog(signal),
    retry: false,
  });
}

/** Author a new agent definition, then refresh the catalog. */
export function useCreateAgentCatalog() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ id, draft }: { id: string; draft: AgentCatalogDraft }) =>
      createAgentCatalog(id, draft),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["agent-catalog"] }),
  });
}

/** Edit an agent definition's models/efforts/etc., then refresh. */
export function useUpdateAgentCatalog() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ id, draft }: { id: string; draft: AgentCatalogDraft }) =>
      updateAgentCatalog(id, draft),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["agent-catalog"] }),
  });
}

/** Remove an agent definition, then refresh. */
export function useDeleteAgentCatalog() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => deleteAgentCatalog(id),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["agent-catalog"] }),
  });
}
