import { loadRemote, registerRemotes } from "@module-federation/runtime";
import {
  SDK_VERSION,
  registerSlot,
  unregisterExtension,
  withActiveExtension,
} from "@lazybones/ext-sdk";
import type {
  ExtensionRecord,
  ExtSdkHandle,
  RemoteMount,
  SlotKind,
  SlotContributionMap,
} from "@lazybones/ext-sdk";
import { request } from "@/lib/api/client";
import { apiBase } from "@/lib/api/config";
import { satisfies } from "./semver";
import { extApi } from "./host-services";
import { extEventStream } from "./event-stream";
import { extToast } from "./toast";
import { extTheme } from "./theme";
import { getExtContext } from "./context-store";

/** The outcome of attempting to load one remote — surfaced in the extensions UI. */
export interface RemoteLoadResult {
  id: string;
  name: string;
  status: "mounted" | "skipped" | "error";
  /** Why it was skipped/failed (version mismatch, untrusted, fetch error, …). */
  reason?: string;
}

/** MF remote names must be JS-identifier-ish and stable. Extension ids contain
 *  hyphens, so derive a safe alias and keep the daemon id for everything else. */
function remoteAlias(id: string): string {
  return "ext_" + id.replace(/[^a-zA-Z0-9_]/g, "_");
}

/** The absolute URL the MF runtime imports a remote's `remoteEntry.js` from —
 *  always the **daemon proxy** (`GET /extensions/:id/frontend/*`, design §4.3),
 *  never a third-party origin. This is also the CSP `script-src` the host must
 *  allow (the daemon origin) so the dynamic `import()` resolves (design §4.4). */
function remoteEntryUrl(ext: ExtensionRecord): string {
  const entry = ext.frontend?.entry ?? "remoteEntry.js";
  return `${apiBase()}/extensions/${ext.id}/frontend/${entry}`;
}

/**
 * TRUST GATE — **first-party / signed-only** (design §4.5, option 1).
 *
 * A federated remote runs as fully-trusted in-origin JS with the live REST
 * client and the operator's session; there is *no* frontend sandbox in v1. So
 * we only ever load remotes the daemon vouches for. Today the gate is "the
 * daemon served it, enabled, from a non-arbitrary source"; once signing lands it
 * tightens to "the embedded signature verifies". This is the single choke point
 * an iframe/postMessage bridge would replace to admit untrusted remotes later —
 * keep all admit/deny logic here.
 */
function isTrusted(ext: ExtensionRecord): boolean {
  if (!ext.enabled) return false;
  // `url`-sourced remotes are arbitrary third-party origins fetched at install;
  // until signature verification exists, do not run them in-origin.
  const sourceKind =
    typeof ext.source === "string" ? ext.source : Object.keys(ext.source)[0];
  return sourceKind === "upload" || sourceKind === "registry";
}

/** Whether the remote's declared SDK range admits the host's SDK version. A
 *  remote with no declared range is allowed but flagged (the host advertises a
 *  pre-1.0 SDK; a remote that pins a range is the well-behaved case). */
function sdkCompatible(ext: ExtensionRecord): { ok: boolean; reason?: string } {
  const range = ext.frontend?.sdk_range ?? null;
  if (!range) return { ok: true };
  if (satisfies(SDK_VERSION, range)) return { ok: true };
  return {
    ok: false,
    reason: `remote needs @lazybones/ext-sdk ${range}, host provides ${SDK_VERSION}`,
  };
}

/** Build the per-extension SDK handle passed to a remote's mount function. */
function makeHandle(ext: ExtensionRecord): ExtSdkHandle {
  return {
    extensionId: ext.id,
    sdkVersion: SDK_VERSION,
    api: extApi,
    events: extEventStream,
    toast: extToast,
    theme: extTheme,
    getContext: getExtContext,
    register: <K extends SlotKind>(slot: K, contribution: SlotContributionMap[K]) =>
      registerSlot(slot, contribution, ext.id),
  };
}

/** Load and mount one remote. Any failure is caught and returned as an `error`
 *  result so one bad remote never aborts the boot sweep (design §4.3). */
async function mountRemote(ext: ExtensionRecord): Promise<RemoteLoadResult> {
  const name = remoteAlias(ext.id);
  if (!isTrusted(ext)) {
    return { id: ext.id, name, status: "skipped", reason: "not first-party/signed (untrusted)" };
  }
  const compat = sdkCompatible(ext);
  if (!compat.ok) {
    return { id: ext.id, name, status: "skipped", reason: compat.reason };
  }

  const exposed = ext.frontend?.exposed_module ?? "./mount";
  try {
    // Clear any prior registrations (re-load after enable/disable) before mount.
    unregisterExtension(ext.id);
    registerRemotes([{ name, entry: remoteEntryUrl(ext) }], { force: true });

    const mod = await loadRemote<{ default: RemoteMount }>(`${name}/${stripDot(exposed)}`);
    const mount = mod?.default;
    if (typeof mount !== "function") {
      return { id: ext.id, name, status: "error", reason: `exposed module ${exposed} has no default mount export` };
    }
    // Stamp attribution so any bare `registerSlot` the remote calls is scoped.
    withActiveExtension(ext.id, () => mount(makeHandle(ext)));
    return { id: ext.id, name, status: "mounted" };
  } catch (err) {
    return {
      id: ext.id,
      name,
      status: "error",
      reason: err instanceof Error ? err.message : String(err),
    };
  }
}

/** MF's `loadRemote('name/module')` does not want the leading `./` of an
 *  exposed-module key. */
function stripDot(exposed: string): string {
  return exposed.replace(/^\.\//, "");
}

/** Fetch the enabled frontend remotes from the daemon and mount each. Returns one
 *  result per candidate. Safe to call repeatedly (e.g. after an install/enable);
 *  each remote re-registers idempotently. */
export async function loadFrontendExtensions(): Promise<RemoteLoadResult[]> {
  let list: ExtensionRecord[];
  try {
    list = await request<ExtensionRecord[]>("/extensions?frontend=1");
  } catch (err) {
    console.error("[ext] failed to list frontend extensions", err);
    return [];
  }
  const candidates = list.filter((e) => e.frontend);
  const results = await Promise.all(candidates.map(mountRemote));
  for (const r of results) {
    if (r.status === "error") {
      console.error(`[ext] remote "${r.id}" failed to mount: ${r.reason}`);
    }
  }
  return results;
}
