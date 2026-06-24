import { createObservable } from "@lazybones/ext-sdk";
import type { ExtContext, ExtRouteContext, ExtTaskContext } from "@lazybones/ext-sdk";

/** The single source of truth for the host context handed to remotes (current
 *  route + open task). The host shell pushes route changes here; the task-detail
 *  panel pushes the open task. Remotes read it through the SDK. */
const store = createObservable<ExtContext>({ route: { view: "dashboard" } });

/** Update the active top-level view (called from the app shell on navigation). */
export function setExtRoute(route: ExtRouteContext): void {
  const cur = store.get();
  if (cur.route.view === route.view) return;
  store.set({ ...cur, route });
}

/** Set or clear the task the operator currently has open. */
export function setExtTask(task: ExtTaskContext | undefined): void {
  const cur = store.get();
  if (cur.task?.taskId === task?.taskId && cur.task?.runId === task?.runId) return;
  store.set({ ...cur, task });
}

export function getExtContext(): ExtContext {
  return store.get();
}

export function subscribeExtContext(handler: (ctx: ExtContext) => void) {
  const emit = () => handler(store.get());
  return store.subscribe(emit);
}
