/**
 * `@lazybones/ext-sdk` — the host SDK for lazybones **frontend extensions**
 * (Module Federation remotes, design §4).
 *
 * A remote default-exports a {@link RemoteMount} from its exposed module. The
 * host calls it once with an {@link ExtSdkHandle}, through which the remote:
 *  - reaches the typed REST client ({@link ExtApiClient}),
 *  - reads the current route/task context ({@link ExtContext}),
 *  - subscribes to the SSE event stream ({@link ExtEventStream}),
 *  - raises toasts ({@link ExtToast}),
 *  - reads theme tokens ({@link ExtTheme}),
 *  - and registers components into named UI slots (`route`, `task-detail.tab`).
 *
 * The host and every remote share this *one* module instance (it is a Module
 * Federation shared singleton), so the slot registry and installed services are
 * global. Remotes depend only on this package — never on host internals — which
 * is the boundary an iframe/postMessage bridge can replace for untrusted
 * remotes later (design §4.5). This v1 is **first-party / signed-only**.
 */

export { SDK_VERSION } from "./version";

// Host wiring (the lazybones app calls these; remotes do not).
export {
  installHostServices,
  getHostServices,
  isHostInstalled,
  subscribeHostServices,
} from "./host";

// Slot registry.
export {
  registerSlot,
  unregisterExtension,
  getSlotContributions,
  subscribeSlots,
  withActiveExtension,
} from "./slots";

// React hooks (used by both the host renderers and remotes).
export {
  useHostServices,
  useExtApi,
  useExtContext,
  useExtEvent,
  useToast,
  useNotify,
  useExtTheme,
  useSlotContributions,
} from "./hooks";

export { createObservable } from "./store";
export type { Observable } from "./store";

export type * from "./types";
