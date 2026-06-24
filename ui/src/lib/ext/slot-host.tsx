import { useSlotContributions } from "@lazybones/ext-sdk";
import type { RegisteredSlot } from "@lazybones/ext-sdk";
import { ExtErrorBoundary } from "./error-boundary";

/** Prefix marking a view id as an extension-contributed `route` (vs a built-in
 *  view). Encodes the owning extension so the host can find the contribution and
 *  attribute errors. */
const ROUTE_PREFIX = "ext:";

/** Build the namespaced view id for an extension route contribution. */
export function extRouteId(extensionId: string, routeId: string): string {
  return `${ROUTE_PREFIX}${extensionId}:${routeId}`;
}

/** Whether a router view id belongs to an extension route. */
export function isExtRoute(view: string): boolean {
  return view.startsWith(ROUTE_PREFIX);
}

/** A built nav entry for an extension route (consumed by the sidebar). */
export interface ExtNavEntry {
  /** The namespaced view id to navigate to. */
  view: string;
  label: string;
  icon?: RegisteredSlot<"route">["icon"];
}

/** All registered extension `route`s as sidebar nav entries. Re-renders as
 *  remotes register/unregister. */
export function useExtRouteNav(): ExtNavEntry[] {
  const routes = useSlotContributions("route");
  return routes.map((r) => ({
    view: extRouteId(r.extensionId, r.id),
    label: r.label,
    icon: r.icon,
  }));
}

/** Render the extension `route` page matching a namespaced view id, wrapped in an
 *  error boundary so a broken page never white-screens the app. Returns a
 *  not-found notice if the view id matches no live contribution (e.g. the
 *  extension was disabled while open). */
export function ExtRouteView({ view }: { view: string }) {
  const routes = useSlotContributions("route");
  const match = routes.find((r) => extRouteId(r.extensionId, r.id) === view);

  if (!match) {
    return (
      <div className="p-8 text-sm text-muted-foreground">
        This extension page is no longer available.
      </div>
    );
  }

  const Page = match.component;
  return (
    <ExtErrorBoundary extensionId={match.extensionId} label="page">
      <Page />
    </ExtErrorBoundary>
  );
}
