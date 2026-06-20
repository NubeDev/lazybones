import { request } from "./client";
import type { AgentCatalog } from "@/types/agent";

/** The authored fields of an agent catalog entry (no id / timestamps). */
export interface AgentCatalogDraft {
  label: string;
  env_var: string;
  login_hint: string;
  models: string[];
  default_model: string | null;
  efforts: string[];
  default_effort: string | null;
}

/** `GET /agent-catalog` — every agent definition (open read). */
export function listAgentCatalog(signal?: AbortSignal): Promise<AgentCatalog[]> {
  return request<AgentCatalog[]>("/agent-catalog", { signal });
}

/** `GET /agent-catalog/:id` — one entry; `404` if absent. */
export function getAgentCatalog(id: string, signal?: AbortSignal): Promise<AgentCatalog> {
  return request<AgentCatalog>(`/agent-catalog/${encodeURIComponent(id)}`, { signal });
}

/** `POST /agent-catalog` — author a new entry. `409` if id taken. */
export function createAgentCatalog(id: string, draft: AgentCatalogDraft): Promise<AgentCatalog> {
  return request<AgentCatalog>("/agent-catalog", {
    method: "POST",
    auth: true,
    body: { id, ...draft },
  });
}

/** `PATCH /agent-catalog/:id` — overwrite the authored fields. */
export function updateAgentCatalog(id: string, draft: AgentCatalogDraft): Promise<AgentCatalog> {
  return request<AgentCatalog>(`/agent-catalog/${encodeURIComponent(id)}`, {
    method: "PATCH",
    auth: true,
    body: draft,
  });
}

/** `DELETE /agent-catalog/:id` — remove an entry; returns whether it existed. */
export function deleteAgentCatalog(id: string): Promise<{ deleted: boolean }> {
  return request<{ deleted: boolean }>(`/agent-catalog/${encodeURIComponent(id)}`, {
    method: "DELETE",
    auth: true,
  });
}
