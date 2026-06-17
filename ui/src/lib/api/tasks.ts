import { request } from "./client";
import type { Status, Task } from "@/types/task";

/** `GET /tasks` — all tasks, or one lifecycle status. */
export function listTasks(status?: Status, signal?: AbortSignal): Promise<Task[]> {
  const q = status ? `?status=${status}` : "";
  return request<Task[]>(`/tasks${q}`, { signal });
}

/** `GET /tasks/:id` — one task with its full spec + claim state. */
export function getTask(id: string, signal?: AbortSignal): Promise<Task> {
  return request<Task>(`/tasks/${encodeURIComponent(id)}`, { signal });
}

/** `POST /tasks/promote` — promote pending tasks whose deps are done → ready. */
export function promoteReady(): Promise<void> {
  return request<void>("/tasks/promote", { method: "POST", auth: true });
}

/** `POST /tasks/:id/block` — force a task to blocked with a reason. */
export function blockTask(id: string, reason: string): Promise<void> {
  return request<void>(`/tasks/${encodeURIComponent(id)}/block`, {
    method: "POST",
    auth: true,
    body: { reason },
  });
}
