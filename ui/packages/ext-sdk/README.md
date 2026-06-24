# @lazybones/ext-sdk

The host SDK for **lazybones frontend extensions** — Module Federation remotes
loaded into the lazybones UI at runtime (design `docs/design/extension-system.md`
§4).

A frontend extension is a separately-built Vite + Module Federation **remote**
that exposes one **mount module**. The lazybones host:

1. fetches the enabled remotes from `GET /extensions?frontend=1`,
2. registers each remote's `remoteEntry.js` (served by the daemon under
   `GET /extensions/:id/frontend/*`) with the MF runtime,
3. imports the remote's exposed module and calls its default export once,
   handing it an [`ExtSdkHandle`].

Through that handle the remote reaches the host's REST client, route/task
context, SSE stream, toasts, theme tokens, and — most importantly — the **slot
registry** it contributes UI into.

> **Trust:** v1 is **first-party / signed-only** (§4.5). A remote runs as
> fully-trusted in-origin JavaScript. The SDK is the *only* surface a remote
> touches the host through, so it is also the seam a future `<iframe sandbox>` +
> `postMessage` bridge replaces to admit untrusted remotes. Do not reach around
> it (no bare `fetch` to the daemon, no direct DOM of the host).

## Shared singletons

The host shares `react`, `react-dom`, `react/jsx-runtime`,
`@tanstack/react-query`, the Radix primitives, and **this SDK** as MF
singletons. A remote must mark the same modules `shared singleton` so it does
**not** bundle its own React or get a private (empty) slot registry.

## Authoring a remote

```tsx
// src/mount.tsx  — the MF-exposed module (frontend.exposed_module = "./mount")
import type { RemoteMount } from "@lazybones/ext-sdk";
import { FileCheck } from "lucide-react";

const mount: RemoteMount = (sdk) => {
  // A top-level page + nav entry.
  sdk.register("route", {
    id: "gate-report",
    label: "Gate Report",
    icon: FileCheck,
    component: () => {
      const ctx = sdk.getContext();
      return <div className="p-6">Gate report for {ctx.route.view}</div>;
    },
  });

  // A tab on the task-detail view.
  sdk.register("task-detail.tab", {
    id: "gate",
    label: "Gate",
    component: ({ taskId }) => <GatePanel taskId={taskId} sdk={sdk} />,
  });
};

export default mount;
```

The remote declares the SDK range it was built against in its
`lazybones.ext.toml` manifest (`frontend.sdk_range`, e.g. `^0.1.0`); the host
refuses to mount a remote whose range its `SDK_VERSION` does not satisfy.

## Surface

- `useExtApi()` / `sdk.api` — typed REST client (`get` / `post` / `request`).
- `useExtContext()` / `sdk.getContext()` — current route + open task.
- `useExtEvent(type, handler)` / `sdk.events` — the daemon SSE stream.
- `useToast()` / `sdk.toast` — operator notifications.
- `useExtTheme()` / `sdk.theme` — theme mode + resolved CSS tokens.
- `useSlotContributions(slot)` — host-side; render what remotes registered.
- `registerSlot(slot, contribution)` / `sdk.register(...)` — contribute UI.

Slots implemented by the v1 host: `route`, `task-detail.tab`. The remaining
`SlotKind`s (`dashboard.widget`, `settings.section`, `task.action`) are typed
and reserved for later host versions.
