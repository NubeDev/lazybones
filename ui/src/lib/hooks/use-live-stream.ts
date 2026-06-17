import { useEffect } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { apiBase } from "@/lib/api/config";

/** Subscribe to `GET /stream` (SSE) and invalidate the task/workflow/run queries
 *  on every `transition` so the board, workflow state, and history advance live
 *  without manual refresh. `activity` events (ephemeral agent progress) also nudge
 *  a refresh so "running" cards feel alive. The polling intervals on those queries
 *  stay as a reconciliation backstop for dropped/lagged frames (per stream.rs).
 *
 *  Mounted once near the app root so a single EventSource serves every view. */
export function useLiveStream() {
  const qc = useQueryClient();

  useEffect(() => {
    const url = `${apiBase()}/stream`;
    let es: EventSource | null = null;
    let closed = false;

    function refresh() {
      qc.invalidateQueries({ queryKey: ["tasks"] });
      qc.invalidateQueries({ queryKey: ["task"] });
      qc.invalidateQueries({ queryKey: ["workflows"] });
      qc.invalidateQueries({ queryKey: ["workflow"] });
      qc.invalidateQueries({ queryKey: ["run"] });
    }

    try {
      es = new EventSource(url);
    } catch {
      return;
    }
    // On (re)connect, reconcile in case we missed frames while disconnected.
    es.addEventListener("open", refresh);
    es.addEventListener("transition", refresh);
    es.addEventListener("activity", refresh);

    return () => {
      closed = true;
      es?.removeEventListener("open", refresh);
      es?.removeEventListener("transition", refresh);
      es?.removeEventListener("activity", refresh);
      es?.close();
      void closed;
    };
  }, [qc]);
}
