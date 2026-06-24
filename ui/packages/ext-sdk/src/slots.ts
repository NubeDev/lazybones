import { createObservable } from "./store";
import type {
  AnyRegisteredSlot,
  RegisteredSlot,
  SlotContributionMap,
  SlotKind,
  Unsubscribe,
} from "./types";

/** The shared slot registry. A singleton (because the SDK is a shared singleton)
 *  array of every contribution every remote has registered, in registration
 *  order. The host renders from it; remotes append to it via `registerSlot`. */
const registry = createObservable<AnyRegisteredSlot[]>([]);

/** Context flag so `registerSlot` can stamp the calling extension's id without
 *  the remote having to pass it on every call. Set by the loader around a
 *  remote's mount; falls back to `"unknown"` outside that window. */
let activeExtensionId: string | null = null;

/** Run `fn` with `extensionId` as the attribution for any `registerSlot` calls
 *  it makes (used by the host loader while invoking a remote's mount). */
export function withActiveExtension<T>(extensionId: string, fn: () => T): T {
  const prev = activeExtensionId;
  activeExtensionId = extensionId;
  try {
    return fn();
  } finally {
    activeExtensionId = prev;
  }
}

/** Register a component into a named host slot. Returns an unsubscribe that
 *  removes the contribution (called on extension teardown / disable).
 *
 *  Remotes normally call this through the `register` method on the SDK handle
 *  passed to their mount function, which scopes `extensionId` automatically; the
 *  free function is exported for advanced/manual use. */
export function registerSlot<K extends SlotKind>(
  slot: K,
  contribution: SlotContributionMap[K],
  extensionId: string = activeExtensionId ?? "unknown",
): Unsubscribe {
  // The runtime shape is exactly `RegisteredSlot<K>`; the cast to the union
  // element type is sound (TS can't prove the generic narrows on its own).
  const entry = { ...contribution, slot, extensionId } as unknown as AnyRegisteredSlot;
  // Registration is idempotent on `(extensionId, slot, contribution id)`: a
  // re-register REPLACES the prior entry rather than appending a duplicate. This
  // keeps the registry clean when a remote's mount runs more than once — e.g.
  // React StrictMode double-invokes the host's mount effect in development, which
  // would otherwise show the same `route`/tab twice.
  const id = (contribution as { id?: string }).id;
  registry.update((prev) => [
    ...prev.filter(
      (e) =>
        !(
          e.extensionId === extensionId &&
          e.slot === slot &&
          (e as { id?: string }).id === id
        ),
    ),
    entry,
  ]);
  return () => {
    registry.update((prev) => prev.filter((e) => e !== entry));
  };
}

/** Remove every contribution registered by one extension (teardown on disable
 *  or reload). */
export function unregisterExtension(extensionId: string): void {
  registry.update((prev) => prev.filter((e) => e.extensionId !== extensionId));
}

/** All contributions registered into one slot, narrowed to that slot's shape. */
export function getSlotContributions<K extends SlotKind>(
  slot: K,
): ReadonlyArray<RegisteredSlot<K>> {
  return registry.get().filter((e) => e.slot === slot) as unknown as ReadonlyArray<
    RegisteredSlot<K>
  >;
}

/** Subscribe to any registry change (add/remove). */
export function subscribeSlots(listener: () => void): Unsubscribe {
  return registry.subscribe(listener);
}

/** Internal store accessor for the React hooks. */
export const _slotStore = registry;
