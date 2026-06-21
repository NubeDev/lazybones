import { useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { ShieldAlert, Check, X } from "lucide-react";
import { Button } from "@/components/ui/button";
import { ApiError } from "@/lib/api/client";
import { confirmAgentAction } from "@/lib/api/agent-chat";
import type { ConfirmAction } from "@/types/agent-chat";

type State =
  | { kind: "pending" }
  | { kind: "running" }
  | { kind: "done" }
  | { kind: "cancelled" }
  | { kind: "error"; message: string };

/** Renders a gated lifecycle action the agent proposed, with Confirm/Cancel.
 *  On Confirm, the UI issues the exact REST call the agent described — under the
 *  operator's loop token, never the agent's (scope §10.2). The agent has no path
 *  to take the action itself; this card is the only way it happens. */
export function AgentConfirmCard({
  summary,
  action,
}: {
  summary: string;
  action: ConfirmAction;
}) {
  const qc = useQueryClient();
  const [state, setState] = useState<State>({ kind: "pending" });

  const confirm = async () => {
    setState({ kind: "running" });
    try {
      await confirmAgentAction({
        method: action.method,
        path: action.path,
        body: action.body,
      });
      setState({ kind: "done" });
      // The action changed lifecycle state — refresh the live views.
      qc.invalidateQueries({ queryKey: ["workflows"] });
      qc.invalidateQueries({ queryKey: ["workflow"] });
      qc.invalidateQueries({ queryKey: ["tasks"] });
      qc.invalidateQueries({ queryKey: ["task"] });
    } catch (e) {
      const message =
        e instanceof ApiError ? e.message : e instanceof Error ? e.message : "call failed";
      setState({ kind: "error", message });
    }
  };

  return (
    <div className="rounded-lg border border-status-blocked/40 bg-status-blocked/5 p-2.5 text-xs">
      <div className="flex items-start gap-2">
        <ShieldAlert className="mt-0.5 size-4 shrink-0 text-status-blocked" />
        <div className="min-w-0 flex-1">
          <p className="font-medium">{summary}</p>
          <p className="mt-0.5 truncate font-mono text-[10px] text-muted-foreground">
            {action.method} {action.path}
          </p>

          {state.kind === "pending" && (
            <div className="mt-2 flex items-center gap-2">
              <Button size="sm" onClick={confirm} className="h-7">
                <Check className="size-3.5" /> Confirm
              </Button>
              <Button
                size="sm"
                variant="ghost"
                onClick={() => setState({ kind: "cancelled" })}
                className="h-7"
              >
                <X className="size-3.5" /> Cancel
              </Button>
            </div>
          )}
          {state.kind === "running" && (
            <p className="mt-2 text-[11px] text-muted-foreground">Running…</p>
          )}
          {state.kind === "done" && (
            <p className="mt-2 inline-flex items-center gap-1 text-[11px] text-status-done">
              <Check className="size-3.5" /> Done.
            </p>
          )}
          {state.kind === "cancelled" && (
            <p className="mt-2 text-[11px] text-muted-foreground">Cancelled.</p>
          )}
          {state.kind === "error" && (
            <p className="mt-2 text-[11px] text-status-blocked">{state.message}</p>
          )}
        </div>
      </div>
    </div>
  );
}
