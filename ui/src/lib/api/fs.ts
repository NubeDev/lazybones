import { request } from "./client";
import type { FsListing } from "@/types/gh";

/** `GET /fs/list?path=` — browse host directories for the repo/dir picker.
 *  Omit `path` to start at `$HOME`. Unguarded (no token needed). */
export function listDir(path?: string, signal?: AbortSignal): Promise<FsListing> {
  const q = path ? `?path=${encodeURIComponent(path)}` : "";
  return request<FsListing>(`/fs/list${q}`, { signal });
}
