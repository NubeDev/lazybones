import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  deleteSecret,
  getEngine,
  listAgents,
  listSecrets,
  putSecret,
  testAgent,
} from "@/lib/api/engine";

/** hcom engine availability (slow-changing; light polling). */
export function useEngine() {
  return useQuery({
    queryKey: ["engine"],
    queryFn: ({ signal }) => getEngine(signal),
    refetchInterval: 15000,
    retry: false,
  });
}

/** Agent CLI install + setup state. */
export function useAgents() {
  return useQuery({
    queryKey: ["agents"],
    queryFn: ({ signal }) => listAgents(signal),
    refetchInterval: 15000,
    retry: false,
  });
}

/** Stored credential metadata (no values). */
export function useSecrets() {
  return useQuery({
    queryKey: ["secrets"],
    queryFn: ({ signal }) => listSecrets(signal),
    retry: false,
  });
}

/** Store/rotate a credential, then refresh the agent + secret views. */
export function usePutSecret() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ tool, envVar, value }: { tool: string; envVar: string; value: string }) =>
      putSecret(tool, envVar, value),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ["secrets"] });
      qc.invalidateQueries({ queryKey: ["agents"] });
    },
  });
}

/** Live-test an agent's credential by launching it through hcom. */
export function useTestAgent() {
  return useMutation({
    mutationFn: (tool: string) => testAgent(tool),
  });
}

/** Remove a credential, then refresh. */
export function useDeleteSecret() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (tool: string) => deleteSecret(tool),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ["secrets"] });
      qc.invalidateQueries({ queryKey: ["agents"] });
    },
  });
}
