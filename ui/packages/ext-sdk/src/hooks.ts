import { useCallback, useEffect, useRef, useState, useSyncExternalStore } from "react";
import { _hostServicesStore, getHostServices } from "./host";
import { _slotStore, getSlotContributions, subscribeSlots } from "./slots";
import type {
  ExtApiClient,
  ExtContext,
  ExtEvent,
  ExtThemeSnapshot,
  HostServices,
  RegisteredSlot,
  SlotKind,
  ToastKind,
} from "./types";

/** Subscribe to the installed {@link HostServices}, re-rendering if the host
 *  (re)installs them. Throws if used before the host has booted. */
export function useHostServices(): HostServices {
  const services = useSyncExternalStore(
    _hostServicesStore.subscribe,
    _hostServicesStore.get,
    _hostServicesStore.get,
  );
  if (!services) return getHostServices(); // throws with a clear message
  return services;
}

/** The typed REST client. */
export function useExtApi(): ExtApiClient {
  return useHostServices().api;
}

/** The live host context (current route + open task). Re-renders on change. */
export function useExtContext(): ExtContext {
  const { getContext, subscribeContext } = useHostServices();
  return useSyncExternalStore(subscribeContext, getContext, getContext);
}

/** Subscribe to one SSE event type (or `"*"`). The handler ref is kept current
 *  so callers don't need to memoize it. */
export function useExtEvent<T = unknown>(
  type: string,
  handler: (event: ExtEvent<T>) => void,
): void {
  const { events } = useHostServices();
  const ref = useRef(handler);
  ref.current = handler;
  useEffect(() => {
    return events.subscribe<T>(type, (e) => ref.current(e));
  }, [events, type]);
}

/** The toast/notify surface. */
export function useToast() {
  return useHostServices().toast;
}

/** Convenience: a single `notify(kind, title, description?)` callback. */
export function useNotify() {
  const toast = useToast();
  return useCallback(
    (kind: ToastKind, title: string, description?: string) =>
      toast.notify({ kind, title, description }),
    [toast],
  );
}

/** The current theme snapshot (mode + resolved tokens); re-renders on change. */
export function useExtTheme(): ExtThemeSnapshot {
  const { theme } = useHostServices();
  const [snap, setSnap] = useState<ExtThemeSnapshot>(() => theme.current());
  useEffect(() => {
    setSnap(theme.current());
    return theme.subscribe(setSnap);
  }, [theme]);
  return snap;
}

/* --- Slot contributions (host-side rendering) ------------------------------ */

// `getSlotContributions` builds a fresh array each call, which would make
// `useSyncExternalStore` loop. Cache the derived array per slot, keyed by the
// underlying registry array reference, so the snapshot is stable until it
// genuinely changes.
const slotCache = new Map<SlotKind, { src: unknown; out: ReadonlyArray<unknown> }>();

function slotSnapshot<K extends SlotKind>(slot: K): ReadonlyArray<RegisteredSlot<K>> {
  const src = _slotStore.get();
  const cached = slotCache.get(slot);
  if (cached && cached.src === src) {
    return cached.out as ReadonlyArray<RegisteredSlot<K>>;
  }
  const out = getSlotContributions(slot);
  slotCache.set(slot, { src, out });
  return out;
}

/** All contributions a remote has registered into a host slot. The host uses
 *  this to render extension `route`s, `task-detail.tab`s, etc. */
export function useSlotContributions<K extends SlotKind>(
  slot: K,
): ReadonlyArray<RegisteredSlot<K>> {
  const subscribe = useCallback((l: () => void) => subscribeSlots(l), []);
  const getSnapshot = useCallback(() => slotSnapshot(slot), [slot]);
  return useSyncExternalStore(subscribe, getSnapshot, getSnapshot);
}
