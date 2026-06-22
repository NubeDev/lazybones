import { request } from "./client";
import type {
  ManagementAgentConfig,
  ManagementAgentDraft,
} from "@/types/management-agent";

/** `GET /settings/management-agent` — the current config, or its default if the
 *  operator has never saved one (open read). */
export function getManagementAgent(
  signal?: AbortSignal,
): Promise<ManagementAgentConfig> {
  return request<ManagementAgentConfig>("/settings/management-agent", { signal });
}

/** `PUT /settings/management-agent` — replace the global config. Requires
 *  `Author`; `400` if the tool is unknown or model/effort isn't in the catalog. */
export function updateManagementAgent(
  draft: ManagementAgentDraft,
): Promise<ManagementAgentConfig> {
  return request<ManagementAgentConfig>("/settings/management-agent", {
    method: "PUT",
    auth: true,
    body: draft,
  });
}

/** `GET /settings/management-agent/workflows/:id` — the *resolved* config for a
 *  workflow (its override if set, else the global default). */
export function getWorkflowManagementAgent(
  workflowId: string,
  signal?: AbortSignal,
): Promise<ManagementAgentConfig> {
  return request<ManagementAgentConfig>(
    `/settings/management-agent/workflows/${encodeURIComponent(workflowId)}`,
    { signal },
  );
}

/** `PUT /settings/management-agent/workflows/:id` — set a per-workflow override. */
export function updateWorkflowManagementAgent(
  workflowId: string,
  draft: ManagementAgentDraft,
): Promise<ManagementAgentConfig> {
  return request<ManagementAgentConfig>(
    `/settings/management-agent/workflows/${encodeURIComponent(workflowId)}`,
    { method: "PUT", auth: true, body: draft },
  );
}

/** `DELETE /settings/management-agent/workflows/:id` — drop a workflow override,
 *  reverting to the global default. */
export function deleteWorkflowManagementAgent(
  workflowId: string,
): Promise<{ deleted: boolean }> {
  return request<{ deleted: boolean }>(
    `/settings/management-agent/workflows/${encodeURIComponent(workflowId)}`,
    { method: "DELETE", auth: true },
  );
}
