import type { Unsubscribe } from "./types";

/** A minimal observable value. Used for the slot registry and the host-context
 *  snapshot so both the host and remotes can subscribe with `useSyncExternal
 *  Store` — no React context needed, which keeps the SDK usable from a remote
 *  even if it is ever moved behind an iframe bridge (design §4.5). */
export interface Observable<T> {
  get(): T;
  set(next: T): void;
  /** Recompute from the current value (handy for immutable array updates). */
  update(fn: (prev: T) => T): void;
  subscribe(listener: () => void): Unsubscribe;
}

export function createObservable<T>(initial: T): Observable<T> {
  let value = initial;
  const listeners = new Set<() => void>();

  return {
    get: () => value,
    set(next) {
      if (Object.is(next, value)) return;
      value = next;
      for (const l of [...listeners]) l();
    },
    update(fn) {
      this.set(fn(value));
    },
    subscribe(listener) {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
  };
}
