import { installHostServices } from "@lazybones/ext-sdk";
import type { ExtApiClient, HostServices } from "@lazybones/ext-sdk";
import { request } from "@/lib/api/client";
import { apiBase } from "@/lib/api/config";
import { getExtContext, subscribeExtContext } from "./context-store";
import { extEventStream } from "./event-stream";
import { extToast } from "./toast";
import { extTheme } from "./theme";

/** The REST client remotes use. It is the host's single `fetch` boundary
 *  (`@/lib/api/client`) re-exposed through the SDK type — a remote never builds
 *  its own URL or token handling, so swapping this for a postMessage bridge
 *  later (untrusted remotes, design §4.5) touches only this object. */
export const extApi: ExtApiClient = {
  baseUrl: () => apiBase(),
  request: (path, opts) => request(path, opts),
  get: (path, opts) => request(path, { ...opts, method: "GET" }),
  post: (path, body, opts) => request(path, { ...opts, method: "POST", body }),
};

const services: HostServices = {
  api: extApi,
  events: extEventStream,
  toast: extToast,
  theme: extTheme,
  getContext: getExtContext,
  subscribeContext: subscribeExtContext,
};

let installed = false;

/** Wire the host implementations into the SDK singleton. Idempotent: safe to
 *  call from a re-rendering provider; only the first call matters. Must run
 *  before any remote is loaded (the loader depends on it). */
export function installExtensionHost(): void {
  if (installed) return;
  installHostServices(services);
  installed = true;
}
