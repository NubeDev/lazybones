import { request } from "./client";

/** `GET /health` — liveness probe used by the connection indicator. */
export async function checkHealth(signal?: AbortSignal): Promise<boolean> {
  try {
    await request<unknown>("/health", { signal });
    return true;
  } catch {
    return false;
  }
}
