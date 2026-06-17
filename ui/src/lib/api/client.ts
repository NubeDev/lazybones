import { apiBase, loopToken } from "./config";

/** A failed REST call, carrying the HTTP status for status-aware UI. */
export class ApiError extends Error {
  constructor(
    public status: number,
    message: string,
  ) {
    super(message);
    this.name = "ApiError";
  }
}

interface RequestOpts {
  method?: "GET" | "POST";
  body?: unknown;
  /** Attach the loop bearer token (required by guarded mutations). */
  auth?: boolean;
  signal?: AbortSignal;
}

/** The single fetch boundary. Every endpoint module builds on this. */
export async function request<T>(path: string, opts: RequestOpts = {}): Promise<T> {
  const { method = "GET", body, auth = false, signal } = opts;
  const headers: Record<string, string> = {};
  if (body !== undefined) headers["content-type"] = "application/json";
  if (auth) headers["authorization"] = `Bearer ${loopToken()}`;

  let res: Response;
  try {
    res = await fetch(`${apiBase()}${path}`, {
      method,
      headers,
      body: body === undefined ? undefined : JSON.stringify(body),
      signal,
    });
  } catch (cause) {
    throw new ApiError(0, `Cannot reach lazybonesd at ${apiBase()}`);
  }

  if (!res.ok) {
    const text = await res.text().catch(() => "");
    throw new ApiError(res.status, text || `${res.status} ${res.statusText}`);
  }

  if (res.status === 204) return undefined as T;
  const ct = res.headers.get("content-type") ?? "";
  if (!ct.includes("application/json")) return undefined as T;
  return (await res.json()) as T;
}
