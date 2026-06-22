import { apiBase, loopToken } from "./config";
import { ApiError, request } from "./client";
import type { Asset } from "@/types/asset";

/** `GET /assets` — list asset metadata (open read), optionally `?project=`. */
export function listAssets(project?: string, signal?: AbortSignal): Promise<Asset[]> {
  const q = project ? `?project=${encodeURIComponent(project)}` : "";
  return request<Asset[]>(`/assets${q}`, { signal });
}

/** The absolute URL serving an asset's bytes — usable directly as an `<img src>`
 *  or download link (`GET /assets/:id`, open read). */
export function assetUrl(id: string): string {
  return `${apiBase()}/assets/${encodeURIComponent(id)}`;
}

/** `POST /assets` — upload a file as a raw body (not multipart). The browser
 *  `File` carries its own `type`/`name`, mapped to `Content-Type`/`X-Filename`.
 *  Content-addressed server-side: identical bytes dedup to one asset. */
export async function uploadAsset(file: File, project?: string): Promise<Asset> {
  const q = project ? `?project=${encodeURIComponent(project)}` : "";
  const res = await fetch(`${apiBase()}/assets${q}`, {
    method: "POST",
    headers: {
      "content-type": file.type || "application/octet-stream",
      "x-filename": file.name,
      authorization: `Bearer ${loopToken()}`,
    },
    body: file,
  });
  if (!res.ok) {
    const text = await res.text().catch(() => "");
    throw new ApiError(res.status, text || `${res.status} ${res.statusText}`);
  }
  return (await res.json()) as Asset;
}

/** `DELETE /assets/:id` — drop an asset's metadata + bytes. */
export function deleteAsset(id: string): Promise<{ deleted: boolean }> {
  return request<{ deleted: boolean }>(`/assets/${encodeURIComponent(id)}`, {
    method: "DELETE",
    auth: true,
  });
}
