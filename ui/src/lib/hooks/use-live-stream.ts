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

    // The hcom log feed maintains its own live state in `useHcomLogFeed` (it
    // appends entries rather than refetching), so a global `hcom_log` listener
    // only needs to reconcile the durable `["hcom", …]` queries that back the
    // per-task drill-in trace.
    function refreshHcom() {
      qc.invalidateQueries({ queryKey: ["hcom"] });
    }

    // A `chat` frame (operator message or mirrored agent reply) reconciles the
    // durable `["chat", …]` conversations the task chat panel renders. Refetch
    // (the rows are the source of truth) rather than appending the frame.
    function refreshChat() {
      qc.invalidateQueries({ queryKey: ["chat"] });
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
    es.addEventListener("hcom_log", refreshHcom);
    es.addEventListener("chat", refreshChat);

    return () => {
      closed = true;
      es?.removeEventListener("open", refresh);
      es?.removeEventListener("transition", refresh);
      es?.removeEventListener("activity", refresh);
      es?.removeEventListener("hcom_log", refreshHcom);
      es?.removeEventListener("chat", refreshChat);
      es?.close();
      void closed;
    };
  }, [qc]);
}
