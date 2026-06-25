import { request } from "./client";
import type { JobReport, SyncStatus } from "@/types/content-sync";

/** `GET /content-sync/status` — where the local checkout stands vs the remote.
 *  Open read; never errors on a network problem (reports `state: "unknown"`). */
export function getSyncStatus(signal?: AbortSignal): Promise<SyncStatus> {
  return request<SyncStatus>("/content-sync/status", { signal });
}

/** `POST /content-sync/pull` — pull the remote and import it into the store.
 *  Requires `Author`. */
export function pullSync(): Promise<JobReport> {
  return request<JobReport>("/content-sync/pull", { method: "POST", auth: true });
}

/** `POST /content-sync/push` — export the store and push it. Requires `Author`. */
export function pushSync(): Promise<JobReport> {
  return request<JobReport>("/content-sync/push", { method: "POST", auth: true });
}
