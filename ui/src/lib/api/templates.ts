import { request } from "./client";
import type { Template } from "@/types/workflow";
import type { WorktreeMode } from "@/types/task";

/** `GET /templates` — all reusable task templates (open read). */
export function listTemplates(signal?: AbortSignal): Promise<Template[]> {
  return request<Template[]>("/templates", { signal });
}

/** `GET /templates/:id` — one template; `404` if absent. */
export function getTemplate(id: string, signal?: AbortSignal): Promise<Template> {
  return request<Template>(`/templates/${encodeURIComponent(id)}`, { signal });
}

/** The authored fields of a template. */
export interface TemplateDraft {
  title: string;
  description: string;
  spec_template: string;
  default_tool: string | null;
  default_model: string | null;
  default_effort: string | null;
  default_worktree_mode: WorktreeMode | null;
}

/** `POST /templates` — author a reusable recipe. `409` if id taken. */
export function createTemplate(id: string, draft: TemplateDraft): Promise<Template> {
  return request<Template>("/templates", {
    method: "POST",
    auth: true,
    body: { id, ...draft },
  });
}

/** `DELETE /templates/:id` — remove a template; returns whether it existed. */
export function deleteTemplate(id: string): Promise<{ deleted: boolean }> {
  return request<{ deleted: boolean }>(`/templates/${encodeURIComponent(id)}`, {
    method: "DELETE",
    auth: true,
  });
}
