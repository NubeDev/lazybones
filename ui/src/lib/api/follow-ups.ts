import { request } from "./client";
import type { FollowUp } from "@/types/follow-up";

/** `GET /runs/:id/follow-ups` — the run's follow-ups, freshest first; optionally
 *  filtered by status (`open` | `resolved`) or task. Read-only, unauthenticated. */
export function getRunFollowUps(
  run: string,
  opts: { status?: string; task?: string } = {},
  signal?: AbortSignal,
): Promise<FollowUp[]> {
  const p = new URLSearchParams();
  if (opts.status) p.set("status", opts.status);
  if (opts.task) p.set("task", opts.task);
  const q = p.toString();
  return request<FollowUp[]>(
    `/runs/${encodeURIComponent(run)}/follow-ups${q ? `?${q}` : ""}`,
    { signal },
  );
}

/** `POST /follow-ups/:id/resolve` — mark one resolved. Operator action (requires
 *  the loop bearer token). */
export function resolveFollowUp(id: string): Promise<FollowUp> {
  return request<FollowUp>(
    `/follow-ups/${encodeURIComponent(id)}/resolve`,
    { method: "POST", auth: true },
  );
}
