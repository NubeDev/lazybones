import { request } from "./client";
import type { Status, Task, WorktreeMode } from "@/types/task";

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

/** `POST /tasks/:id/ready` — promote exactly one task `pending → ready`. */
export function readyTask(id: string): Promise<Task> {
  return request<Task>(`/tasks/${encodeURIComponent(id)}/ready`, {
    method: "POST",
    auth: true,
  });
}

/** `POST /tasks/:id/block` — force a task to blocked with a reason. */
export function blockTask(id: string, reason: string): Promise<void> {
  return request<void>(`/tasks/${encodeURIComponent(id)}/block`, {
    method: "POST",
    auth: true,
    body: { reason },
  });
}

/** The authored fields of a task — shared by create + update. */
export interface TaskDraft {
  title: string;
  spec: string;
  deps: string[];
  owns: string[];
  tool: string | null;
  worktree_mode: WorktreeMode;
}

/** `POST /tasks` — author a new task (starts `pending`). `409` if id taken. */
export function createTask(id: string, draft: TaskDraft): Promise<Task> {
  return request<Task>("/tasks", {
    method: "POST",
    auth: true,
    body: { id, ...draft },
  });
}

/** `PATCH /tasks/:id` — overwrite a task's authored fields; lifecycle kept. */
export function updateTask(id: string, draft: TaskDraft): Promise<Task> {
  return request<Task>(`/tasks/${encodeURIComponent(id)}`, {
    method: "PATCH",
    auth: true,
    body: draft,
  });
}

/** `DELETE /tasks/:id` — remove a task and its dependency edges. */
export function deleteTask(id: string): Promise<void> {
  return request<void>(`/tasks/${encodeURIComponent(id)}`, {
    method: "DELETE",
    auth: true,
  });
}
