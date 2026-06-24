import type { ComponentType, ReactNode } from "react";

/* ------------------------------------------------------------------ *
 *  Backend wire shapes (mirror of the daemon's `Extension` JSON).
 *  Kept here so remotes type their own descriptors without importing
 *  host internals (the whole point of the SDK boundary, design §4.5).
 * ------------------------------------------------------------------ */

/** The frontend half of an extension — a Module Federation remote. Mirrors the
 *  daemon `FrontendDescriptor` (lazybones-store) returned by `GET /extensions`. */
export interface FrontendDescriptor {
  /** Path within the served bundle of the federated `remoteEntry.js`. */
  entry: string;
  /** The MF *exposed module* the host imports to obtain the remote's mount
   *  entry point (e.g. `./mount`). */
  exposed_module: string;
  /** Compatible `@lazybones/ext-sdk` semver range the remote was built against;
   *  `null`/absent = unspecified (mounted, but flagged). */
  sdk_range?: string | null;
  /** The named UI slots this remote registers into. */
  slots?: string[];
}

/** Install provenance discriminant returned in the `source` field. */
export type ExtensionSourceKind = "upload" | "url" | "registry";

/** One installed extension as returned by `GET /extensions`. Only the fields the
 *  frontend host needs are typed precisely; the rest are carried opaquely. */
export interface ExtensionRecord {
  id: string;
  name: string;
  version: string;
  wit_world: string;
  exports: string[];
  requested_caps: string[];
  granted_caps: string[];
  wasm_sha256: string;
  enabled: boolean;
  /** Serde-tagged `ExtensionSource`: `"upload"` or `{ url: "…" }` / `{ registry: "…" }`. */
  source: ExtensionSourceKind | Record<string, string>;
  frontend?: FrontendDescriptor | null;
  created_at: string;
}

/* ------------------------------------------------------------------ *
 *  Host context — where the operator is right now.
 * ------------------------------------------------------------------ */

/** The current top-level page the operator is viewing. */
export interface ExtRouteContext {
  /** The active view id (a built-in view, or an extension `route` id). */
  view: string;
}

/** The task the operator currently has open, if any (drives `task-detail.tab`). */
export interface ExtTaskContext {
  taskId: string;
  runId?: string;
}

/** The live host context handed to remotes. Immutable snapshot; subscribe via
 *  {@link HostServices.subscribeContext} or the `useExtContext` hook. */
export interface ExtContext {
  route: ExtRouteContext;
  task?: ExtTaskContext;
}

/* ------------------------------------------------------------------ *
 *  REST client.
 * ------------------------------------------------------------------ */

/** Options for a single REST call. Mirrors the host's `request` boundary so an
 *  extension never reaches around the SDK to `fetch` the daemon directly — that
 *  indirection is what lets an iframe bridge replace it later (design §4.5). */
export interface ExtRequestOpts {
  method?: "GET" | "POST" | "PUT" | "PATCH" | "DELETE";
  body?: unknown;
  /** Attach the loop bearer token (required by guarded mutations). */
  auth?: boolean;
  signal?: AbortSignal;
}

/** The typed REST client surface the host exposes to remotes. */
export interface ExtApiClient {
  /** The daemon base URL all extension asset/REST paths resolve against. */
  baseUrl(): string;
  /** Issue a REST call against the daemon and decode JSON (or `undefined`). */
  request<T>(path: string, opts?: ExtRequestOpts): Promise<T>;
  get<T>(path: string, opts?: Omit<ExtRequestOpts, "method" | "body">): Promise<T>;
  post<T>(path: string, body?: unknown, opts?: Omit<ExtRequestOpts, "method" | "body">): Promise<T>;
}

/* ------------------------------------------------------------------ *
 *  SSE event stream.
 * ------------------------------------------------------------------ */

/** A frame from the daemon `GET /stream` SSE feed, normalised to `{type,data}`. */
export interface ExtEvent<T = unknown> {
  /** The SSE event name (`transition`, `activity`, `hcom_log`, `chat`, …). */
  type: string;
  /** The parsed JSON payload, or the raw string if it was not JSON. */
  data: T;
}

/** Unsubscribe handle returned by every `subscribe*` call. */
export type Unsubscribe = () => void;

/** The shared SSE stream. A single `EventSource` is multiplexed across all
 *  subscribers (host + every remote) so an extension never opens its own. */
export interface ExtEventStream {
  /** Subscribe to one event type (or `"*"` for all). Returns an unsubscribe. */
  subscribe<T = unknown>(type: string, handler: (event: ExtEvent<T>) => void): Unsubscribe;
}

/* ------------------------------------------------------------------ *
 *  Toast / notify.
 * ------------------------------------------------------------------ */

export type ToastKind = "info" | "success" | "error";

export interface ToastOptions {
  title: string;
  description?: string;
  kind?: ToastKind;
  /** Auto-dismiss after N ms; `0` keeps it until dismissed. Defaults to ~5s. */
  durationMs?: number;
}

/** Operator notifications. The host owns rendering; remotes only enqueue. */
export interface ExtToast {
  notify(opts: ToastOptions): void;
  info(title: string, description?: string): void;
  success(title: string, description?: string): void;
  error(title: string, description?: string): void;
}

/* ------------------------------------------------------------------ *
 *  Theme tokens.
 * ------------------------------------------------------------------ */

export type ThemeMode = "dark" | "light";

/** A snapshot of the design-system theme. `tokens` are the resolved CSS custom
 *  properties (e.g. `--accent`) so a remote can read colors without guessing.
 *  Remotes should prefer the shared Tailwind/Radix classes for styling; tokens
 *  are the escape hatch for canvas/SVG/inline-style surfaces. */
export interface ExtThemeSnapshot {
  mode: ThemeMode;
  tokens: Record<string, string>;
}

export interface ExtTheme {
  current(): ExtThemeSnapshot;
  subscribe(handler: (snapshot: ExtThemeSnapshot) => void): Unsubscribe;
}

/* ------------------------------------------------------------------ *
 *  Slot contributions — what a remote registers into the host.
 * ------------------------------------------------------------------ */

/** The named slots the host renders. Mirror of the backend extension points
 *  (design §4.2). v1 implements `route` and `task-detail.tab`; the others are
 *  reserved so a remote can be forward-written. */
export type SlotKind =
  | "route"
  | "task-detail.tab"
  | "dashboard.widget"
  | "settings.section"
  | "task.action";

/** A small icon component (lucide-compatible) a contribution may supply. */
export type SlotIcon = ComponentType<{ className?: string }>;

/** A top-level page + nav entry contributed by a remote. */
export interface RouteContribution {
  /** Stable id, unique within the extension; namespaced to `ext:<extId>:<id>`. */
  id: string;
  /** Sidebar label. */
  label: string;
  /** Optional sidebar icon. */
  icon?: SlotIcon;
  /** The page component, rendered in the main content area. */
  component: ComponentType;
}

/** A tab on the task-detail view contributed by a remote. */
export interface TaskTabContribution {
  id: string;
  label: string;
  icon?: SlotIcon;
  /** Rendered inside the tab; receives the open task's context. */
  component: ComponentType<{ taskId: string; runId?: string }>;
}

/** A dashboard card (reserved — not yet rendered by the v1 host). */
export interface WidgetContribution {
  id: string;
  title?: string;
  component: ComponentType;
}

/** A settings panel (reserved — not yet rendered by the v1 host). */
export interface SettingsSectionContribution {
  id: string;
  label: string;
  component: ComponentType;
}

/** A task action button/menu item (reserved — not yet rendered by the v1 host). */
export interface TaskActionContribution {
  id: string;
  label: string;
  icon?: SlotIcon;
  run(ctx: { taskId: string }): void;
}

/** Maps each slot kind to the shape a remote registers into it. */
export interface SlotContributionMap {
  route: RouteContribution;
  "task-detail.tab": TaskTabContribution;
  "dashboard.widget": WidgetContribution;
  "settings.section": SettingsSectionContribution;
  "task.action": TaskActionContribution;
}

/** A registered contribution into one specific slot, decorated by the SDK with
 *  its owning extension id (used for namespacing, error attribution, and
 *  teardown on disable). */
export type RegisteredSlot<K extends SlotKind> = SlotContributionMap[K] & {
  /** The extension that registered this contribution. */
  readonly extensionId: string;
  /** The slot it was registered into. */
  readonly slot: K;
};

/** Any registered contribution — the **union** over all slot kinds (what the
 *  registry array actually holds). Narrow it to a specific [`RegisteredSlot<K>`]
 *  by its `slot` discriminant. */
export type AnyRegisteredSlot = {
  [K in SlotKind]: RegisteredSlot<K>;
}[SlotKind];

/* ------------------------------------------------------------------ *
 *  Host services — the single object the host installs into the SDK.
 * ------------------------------------------------------------------ */

/** The implementations the host wires into the SDK at boot. Remotes never see
 *  this object directly; they reach it through the SDK hooks/functions. Keeping
 *  it a single installable interface is the seam an iframe/postMessage bridge
 *  replaces later for untrusted remotes (design §4.5). */
export interface HostServices {
  api: ExtApiClient;
  events: ExtEventStream;
  toast: ExtToast;
  theme: ExtTheme;
  /** Snapshot of the current host context. */
  getContext(): ExtContext;
  /** Subscribe to host-context changes; fires with the latest snapshot. */
  subscribeContext(handler: (ctx: ExtContext) => void): Unsubscribe;
}

/** What a remote's exposed mount module must default-export: a function the host
 *  calls once after load, passing the SDK handle. The remote registers its
 *  slots inside. Returning a cleanup runs when the extension is unmounted. */
export type RemoteMount = (sdk: ExtSdkHandle) => void | (() => void);

/** The handle passed to a remote's {@link RemoteMount}. Bundles the services the
 *  remote is most likely to capture eagerly, plus `register` so a remote can
 *  contribute slots without importing the registry singleton itself. */
export interface ExtSdkHandle {
  extensionId: string;
  sdkVersion: string;
  api: ExtApiClient;
  events: ExtEventStream;
  toast: ExtToast;
  theme: ExtTheme;
  getContext(): ExtContext;
  register<K extends SlotKind>(slot: K, contribution: SlotContributionMap[K]): Unsubscribe;
}

/** Re-exported so consumers can annotate icons/children without pulling React. */
export type { ReactNode };
