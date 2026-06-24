import type { ExtEvent, ExtEventStream, Unsubscribe } from "@lazybones/ext-sdk";
import { apiBase } from "@/lib/api/config";

/** The event names the daemon `/stream` SSE feed emits today. A `"*"` subscriber
 *  attaches DOM listeners for all of them (EventSource has no "any event" hook). */
const KNOWN_EVENT_TYPES = ["open", "transition", "activity", "hcom_log", "chat"];

/** A single multiplexed `EventSource` over the daemon `GET /stream` feed, shared
 *  by every extension that subscribes (design §4.1 — remotes never open their
 *  own connection). The host's own `useLiveStream` keeps its separate connection
 *  for query invalidation; this one is the extension-facing surface so the SDK
 *  boundary stays the only thing a future iframe bridge must marshal. */
class SharedEventStream implements ExtEventStream {
  private es: EventSource | null = null;
  /** type → set of handlers. `"*"` receives every frame. */
  private readonly handlers = new Map<string, Set<(e: ExtEvent) => void>>();
  /** The DOM listener we attached per concrete event type, for teardown. */
  private readonly domListeners = new Map<string, (ev: MessageEvent) => void>();

  subscribe<T>(type: string, handler: (event: ExtEvent<T>) => void): Unsubscribe {
    this.ensureOpen();
    let set = this.handlers.get(type);
    if (!set) {
      set = new Set();
      this.handlers.set(type, set);
      if (type === "*") KNOWN_EVENT_TYPES.forEach((t) => this.attach(t));
      else this.attach(type);
    }
    set.add(handler as (e: ExtEvent) => void);
    return () => {
      set?.delete(handler as (e: ExtEvent) => void);
    };
  }

  private ensureOpen(): void {
    if (this.es) return;
    try {
      this.es = new EventSource(`${apiBase()}/stream`);
    } catch {
      this.es = null;
    }
  }

  /** Attach a DOM listener for a concrete SSE event name, fanning out to both the
   *  type-specific subscribers and the `"*"` wildcard subscribers. */
  private attach(type: string): void {
    if (!this.es || this.domListeners.has(type)) return;
    const listener = (ev: MessageEvent) => this.dispatch(type, ev.data);
    this.domListeners.set(type, listener);
    this.es.addEventListener(type, listener);
  }

  private dispatch(type: string, raw: string): void {
    let data: unknown = raw;
    try {
      data = JSON.parse(raw);
    } catch {
      /* keep the raw string */
    }
    const event: ExtEvent = { type, data };
    for (const h of this.handlers.get(type) ?? []) safe(h, event);
    for (const h of this.handlers.get("*") ?? []) safe(h, event);
  }
}

function safe(handler: (e: ExtEvent) => void, event: ExtEvent): void {
  try {
    handler(event);
  } catch (err) {
    // A remote's bad handler must never break the stream for everyone.
    console.error("[ext] event handler threw", err);
  }
}

/** The process-wide shared stream singleton. */
export const extEventStream: ExtEventStream = new SharedEventStream();
