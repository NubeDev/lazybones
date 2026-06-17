import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  blockTask,
  createTask,
  deleteTask,
  getTask,
  listTasks,
  promoteReady,
  readyTask,
  updateTask,
  type TaskDraft,
} from "@/lib/api/tasks";
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

/** Promote one task `pending → ready` (the per-card board action). */
export function useReadyTask() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => readyTask(id),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ["tasks"] });
      qc.invalidateQueries({ queryKey: ["task"] });
    },
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

/** Refresh both the board list and any open detail after an authoring change. */
function invalidateTasks(qc: ReturnType<typeof useQueryClient>) {
  qc.invalidateQueries({ queryKey: ["tasks"] });
  qc.invalidateQueries({ queryKey: ["task"] });
}

/** Author a new task (`POST /tasks`). */
export function useCreateTask() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ id, draft }: { id: string; draft: TaskDraft }) =>
      createTask(id, draft),
    onSuccess: () => invalidateTasks(qc),
  });
}

/** Edit a task's authored fields (`PATCH /tasks/:id`). */
export function useUpdateTask() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ id, draft }: { id: string; draft: TaskDraft }) =>
      updateTask(id, draft),
    onSuccess: () => invalidateTasks(qc),
  });
}

/** Delete a task (`DELETE /tasks/:id`). */
export function useDeleteTask() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => deleteTask(id),
    onSuccess: () => invalidateTasks(qc),
  });
}
