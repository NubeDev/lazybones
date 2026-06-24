import { createObservable } from "./store";
import type { HostServices } from "./types";

/** Module-level singleton holding the host's wired services. The host calls
 *  {@link installHostServices} once at boot; remotes (and the SDK hooks) read it
 *  via {@link getHostServices}. Because `@lazybones/ext-sdk` is a Module
 *  Federation **shared singleton**, every remote sees the *same* module instance
 *  and therefore the *same* installed services — that is the whole reason the
 *  SDK is shared rather than bundled per-remote (design §4.1). */
const services = createObservable<HostServices | null>(null);

/** Install the host implementations. Idempotent-ish: re-installing replaces the
 *  services (used by hot-reload in dev). Throws nothing. */
export function installHostServices(impl: HostServices): void {
  services.set(impl);
}

/** Whether the host has wired the SDK yet. */
export function isHostInstalled(): boolean {
  return services.get() !== null;
}

/** Read the installed host services, or throw a clear error if a remote tried to
 *  use the SDK before the host booted (a programming error — the host installs
 *  before it mounts any remote). */
export function getHostServices(): HostServices {
  const s = services.get();
  if (!s) {
    throw new Error(
      "[@lazybones/ext-sdk] host services not installed — the SDK was used " +
        "before the lazybones host called installHostServices().",
    );
  }
  return s;
}

/** Subscribe to host (re)installation — lets hooks recover if the host wires the
 *  SDK after a component using it has already mounted. */
export function subscribeHostServices(listener: () => void) {
  return services.subscribe(listener);
}

/** Internal accessor for the observable (used by hooks' `useSyncExternalStore`). */
export const _hostServicesStore = services;
