import { request } from "./client";
import type {
  AgentReport,
  AgentTestResult,
  EngineReport,
  SecretMeta,
} from "@/types/agent";

/** `GET /engine` — hcom availability (unguarded). */
export function getEngine(signal?: AbortSignal): Promise<EngineReport> {
  return request<EngineReport>("/engine", { signal });
}

/** `GET /agents` — agent CLI install + setup state (loop-guarded). */
export function listAgents(signal?: AbortSignal): Promise<AgentReport[]> {
  return request<AgentReport[]>("/agents", { auth: true, signal });
}

/** `POST /agents/:tool/test` — live-test the agent's credential by launching it. */
export function testAgent(tool: string, signal?: AbortSignal): Promise<AgentTestResult> {
  return request<AgentTestResult>(`/agents/${encodeURIComponent(tool)}/test`, {
    method: "POST",
    auth: true,
    signal,
  });
}

/** `GET /secrets` — stored credential metadata, never the values. */
export function listSecrets(signal?: AbortSignal): Promise<SecretMeta[]> {
  return request<SecretMeta[]>("/secrets", { auth: true, signal });
}

/** `PUT /secrets/:tool` — seal + store an agent CLI credential. */
export function putSecret(
  tool: string,
  envVar: string,
  value: string,
): Promise<SecretMeta> {
  return request<SecretMeta>(`/secrets/${encodeURIComponent(tool)}`, {
    method: "PUT",
    auth: true,
    body: { env_var: envVar, value },
  });
}

/** `DELETE /secrets/:tool` — remove a stored credential. */
export function deleteSecret(tool: string): Promise<void> {
  return request<void>(`/secrets/${encodeURIComponent(tool)}`, {
    method: "DELETE",
    auth: true,
  });
}
