import { request } from "./client";
import type { RunEvent } from "@/types/event";

/** `GET /runs/:id` — the full transition history for a run, oldest first. */
export function runHistory(run: string, signal?: AbortSignal): Promise<RunEvent[]> {
  return request<RunEvent[]>(`/runs/${encodeURIComponent(run)}`, { signal });
}
