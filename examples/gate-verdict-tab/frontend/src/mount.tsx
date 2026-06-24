// The MF-exposed mount module (`frontend.exposed-module = "./mount"`).
//
// The lazybones host imports this module after fetching the remote and calls its
// default export ONCE, handing it an `ExtSdkHandle`. We use the handle to
// register a tab into the `task-detail.tab` slot; the host renders that tab on
// the task-detail view, passing the open task's `{ taskId, runId }`.
import type { RemoteMount } from "@lazybones/ext-sdk";
import { ShieldCheck } from "lucide-react";
import { GateVerdictPanel } from "./GateVerdictPanel";

const mount: RemoteMount = (sdk) => {
  const unregister = sdk.register("task-detail.tab", {
    id: "gate-verdict",
    label: "Gate",
    icon: ShieldCheck,
    // `sdk` is captured so the panel can reach the REST client and this
    // extension's own id (needed to address `POST /extensions/:id/invoke`).
    component: ({ taskId }) => <GateVerdictPanel sdk={sdk} taskId={taskId} />,
  });

  // Returning a cleanup lets the host tear the tab down on disable/uninstall.
  return unregister;
};

export default mount;
