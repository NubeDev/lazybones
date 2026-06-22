import { request } from "./client";
import type { HcomLogEntry, HcomLogKind, RunEvent } from "@/types/event";

/** `GET /runs/:id` — the full transition history for a run, oldest first. */
export function runHistory(run: string, signal?: AbortSignal): Promise<RunEvent[]> {
  return request<RunEvent[]>(`/runs/${encodeURIComponent(run)}`, { signal });
}

/** Server-side filters for the hcom log. `after`/`limit` page forward from a
 *  cursor; omit for the full (oldest-first) log. */
export interface HcomLogQuery {
  task?: string;
  kind?: HcomLogKind;
  after?: number;
  limit?: number;
}

function hcomQuery(q: HcomLogQuery = {}): string {
  const p = new URLSearchParams();
  if (q.task) p.set("task", q.task);
  if (q.kind) p.set("kind", q.kind);
  if (q.after !== undefined) p.set("after", String(q.after));
  if (q.limit !== undefined) p.set("limit", String(q.limit));
  const s = p.toString();
  return s ? `?${s}` : "";
}

/** `GET /runs/:id/hcom` — the run's hcom log, oldest first, optionally filtered
 *  by task/kind and paged via after/limit. */
export function getRunHcomLog(
  run: string,
  q: HcomLogQuery = {},
  signal?: AbortSignal,
): Promise<HcomLogEntry[]> {
  return request<HcomLogEntry[]>(
    `/runs/${encodeURIComponent(run)}/hcom${hcomQuery(q)}`,
    { signal },
  );
}

/** `GET /tasks/:id/hcom` — one agent's full hcom trace (its run, filtered to
 *  this task). */
export function getTaskHcomLog(
  task: string,
  signal?: AbortSignal,
): Promise<HcomLogEntry[]> {
  return request<HcomLogEntry[]>(`/tasks/${encodeURIComponent(task)}/hcom`, {
    signal,
  });
}

/** `GET /tasks/:id/transcript` — on-demand deep transcript passthrough (large;
 *  fetched live, not stored). Returns hcom's raw `--json --full` transcript as an
 *  opaque value, rendered as formatted JSON in the deep view. */
export function getTaskTranscript(
  task: string,
  signal?: AbortSignal,
): Promise<unknown> {
  return request<unknown>(`/tasks/${encodeURIComponent(task)}/transcript`, {
    signal,
  });
}
