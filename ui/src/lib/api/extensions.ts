import { apiBase, loopToken } from "./config";
import { ApiError, request } from "./client";
import type { ExtensionRecord } from "@lazybones/ext-sdk";

/** One installed extension as returned by the daemon. Re-exported from the SDK
 *  wire types so the management UI and the host loader share one shape. */
export type { ExtensionRecord } from "@lazybones/ext-sdk";

/** `GET /extensions` — every installed extension (open read). */
export function listExtensions(signal?: AbortSignal): Promise<ExtensionRecord[]> {
  return request<ExtensionRecord[]>("/extensions", { signal });
}

/** `GET /extensions/:id` — one extension; `404` if absent (open read). */
export function getExtension(id: string, signal?: AbortSignal): Promise<ExtensionRecord> {
  return request<ExtensionRecord>(`/extensions/${encodeURIComponent(id)}`, { signal });
}

/** `POST /extensions` with raw `.wasm` bytes. The daemon distinguishes upload
 *  from URL-install by content-type, so an upload can't ride the JSON `request`
 *  wrapper — it posts the component bytes with `application/wasm` directly.
 *  `id` is optional; absent, the daemon derives `ext-<sha256[..16]>`. */
export async function uploadExtension(file: File, id?: string): Promise<ExtensionRecord> {
  const query = id ? `?id=${encodeURIComponent(id)}` : "";
  let res: Response;
  try {
    res = await fetch(`${apiBase()}/extensions${query}`, {
      method: "POST",
      headers: {
        "content-type": "application/wasm",
        authorization: `Bearer ${loopToken()}`,
      },
      body: file,
    });
  } catch {
    throw new ApiError(0, `Cannot reach lazybonesd at ${apiBase()}`);
  }
  if (!res.ok) {
    const text = await res.text().catch(() => "");
    throw new ApiError(res.status, text || `${res.status} ${res.statusText}`);
  }
  return (await res.json()) as ExtensionRecord;
}

/** `POST /extensions` with `{ url }` — the daemon fetches the `.wasm` itself. */
export function installFromUrl(url: string, id?: string): Promise<ExtensionRecord> {
  const query = id ? `?id=${encodeURIComponent(id)}` : "";
  return request<ExtensionRecord>(`/extensions${query}`, {
    method: "POST",
    auth: true,
    body: { url },
  });
}

/** `DELETE /extensions/:id` — uninstall; returns whether it existed. */
export function deleteExtension(id: string): Promise<{ deleted: boolean }> {
  return request<{ deleted: boolean }>(`/extensions/${encodeURIComponent(id)}`, {
    method: "DELETE",
    auth: true,
  });
}

/** `POST /extensions/:id/enable` — start dispatching to the extension. */
export function enableExtension(id: string): Promise<ExtensionRecord> {
  return request<ExtensionRecord>(`/extensions/${encodeURIComponent(id)}/enable`, {
    method: "POST",
    auth: true,
    body: {},
  });
}

/** `POST /extensions/:id/disable` — stop dispatching. */
export function disableExtension(id: string): Promise<ExtensionRecord> {
  return request<ExtensionRecord>(`/extensions/${encodeURIComponent(id)}/disable`, {
    method: "POST",
    auth: true,
    body: {},
  });
}

/** `POST /extensions/:id/grants` — set the granted capability subset. The daemon
 *  enforces `granted ⊆ requested`. */
export function setGrants(id: string, grantedCaps: string[]): Promise<ExtensionRecord> {
  return request<ExtensionRecord>(`/extensions/${encodeURIComponent(id)}/grants`, {
    method: "POST",
    auth: true,
    body: { granted_caps: grantedCaps },
  });
}
