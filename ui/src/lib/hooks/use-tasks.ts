import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { blockTask, getTask, listTasks, promoteReady } from "@/lib/api/tasks";
import type { Status } from "@/types/task";

/** Poll the full task list. lazybonesd has no SSE feed yet, so we refetch. */
export function useTasks(status?: Status) {
  return useQuery({
    queryKey: ["tasks", status ?? "all"],
    queryFn: ({ signal }) => listTasks(status, signal),
    refetchInterval: 4000,
  });
}

/** A single task's detail (spec + claim state). */
export function useTask(id: string | null) {
  return useQuery({
    queryKey: ["task", id],
    queryFn: ({ signal }) => getTask(id!, signal),
    enabled: !!id,
    refetchInterval: 4000,
  });
}

/** Promote ready tasks, then refresh every task view. */
export function usePromote() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: promoteReady,
    onSuccess: () => qc.invalidateQueries({ queryKey: ["tasks"] }),
  });
}

/** Block a task with a reason. */
export function useBlock() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ id, reason }: { id: string; reason: string }) =>
      blockTask(id, reason),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ["tasks"] });
      qc.invalidateQueries({ queryKey: ["task"] });
    },
  });
}
