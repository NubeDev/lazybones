/** Where `lazybonesd` listens. Overridable at build time via `VITE_API_BASE`,
 *  and at runtime via `localStorage["lazybones-api-base"]` (settings panel). */
const DEFAULT_BASE = "http://127.0.0.1:46787";

const STORAGE_KEY = "lazybones-api-base";

export function apiBase(): string {
  if (typeof localStorage !== "undefined") {
    const override = localStorage.getItem(STORAGE_KEY);
    if (override) return override.replace(/\/$/, "");
  }
  const env = import.meta.env.VITE_API_BASE as string | undefined;
  return (env || DEFAULT_BASE).replace(/\/$/, "");
}

export function setApiBase(base: string): void {
  localStorage.setItem(STORAGE_KEY, base.replace(/\/$/, ""));
}

/** The loop's bearer token used for guarded mutations (claim/promote/sync). */
const TOKEN_KEY = "lazybones-loop-token";
const DEFAULT_TOKEN = "lazybones-loop";

export function loopToken(): string {
  if (typeof localStorage !== "undefined") {
    return localStorage.getItem(TOKEN_KEY) || DEFAULT_TOKEN;
  }
  return DEFAULT_TOKEN;
}

export function setLoopToken(token: string): void {
  localStorage.setItem(TOKEN_KEY, token);
}
