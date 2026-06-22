import { useEffect, useRef, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { apiBase } from "@/lib/api/config";
import { getRunHcomLog, getTaskHcomLog, getTaskTranscript } from "@/lib/api/runs";
import type { HcomLogEntry, HcomLogKind } from "@/types/event";

export interface HcomFilter {
  /** Restrict to one task id, or `null` for all tasks in the run. */
  task: string | null;
  /** Restrict to one kind, or `null` for all kinds. */
  kind: HcomLogKind | null;
}

interface FeedState {
  entries: HcomLogEntry[];
  isLoading: boolean;
  error: unknown;
  /** False between (re)connect and the first successful re-seed. */
  connected: boolean;
}

/** Does a live entry match the active filter? Mirrors the server-side `?task=`
 *  / `?kind=` predicates so the live edge respects the same scope as the seed. */
function matches(e: HcomLogEntry, f: HcomFilter): boolean {
  if (f.task && e.task !== f.task) return false;
  if (f.kind && e.kind !== f.kind) return false;
  return true;
}

/** The Logs feed for one run: seeds from `GET /runs/:id/hcom` under the active
 *  filter, then appends live `hcom_log` SSE entries that match it. A lagging or
 *  reconnecting client re-seeds from the durable log rather than trusting the
 *  live feed to be complete (docs/hcom-logs-scope.md, "SSE"). Refetches whenever
 *  the run or filter changes. */
export function useHcomLogFeed(run: string | null, filter: HcomFilter): FeedState {
  const [state, setState] = useState<FeedState>({
    entries: [],
    isLoading: !!run,
    error: null,
    connected: false,
  });
  // Keep the latest filter readable from inside the long-lived SSE handler
  // without re-subscribing on every keystroke.
  const filterRef = useRef(filter);
  filterRef.current = filter;
  // De-dupe across seed + live and across re-seeds (the `(run, hcom_id)` upsert
  // means an entry can arrive from both paths).
  const seen = useRef<Set<number>>(new Set());

  useEffect(() => {
    if (!run) {
      setState({ entries: [], isLoading: false, error: null, connected: false });
      return;
    }
    let cancelled = false;
    let es: EventSource | null = null;

    async function seed() {
      const f = filterRef.current;
      setState((s) => ({ ...s, isLoading: true, error: null }));
      try {
        const rows = await getRunHcomLog(run!, {
          task: f.task ?? undefined,
          kind: f.kind ?? undefined,
        });
        if (cancelled) return;
        seen.current = new Set(rows.map((r) => r.hcom_id));
        setState({ entries: rows, isLoading: false, error: null, connected: true });
      } catch (error) {
        if (cancelled) return;
        setState((s) => ({ ...s, isLoading: false, error, connected: false }));
      }
    }

    void seed();

    try {
      es = new EventSource(`${apiBase()}/stream`);
    } catch {
      return () => {
        cancelled = true;
      };
    }

    // On (re)connect the live feed may have advanced past us while we were gone;
    // the durable log is the source of truth, so re-seed instead of trusting it.
    es.addEventListener("open", () => void seed());

    es.addEventListener("hcom_log", (ev) => {
      let entry: HcomLogEntry;
      try {
        entry = JSON.parse((ev as MessageEvent).data) as HcomLogEntry;
      } catch {
        return;
      }
      if (entry.run !== run) return;
      if (!matches(entry, filterRef.current)) return;
      if (seen.current.has(entry.hcom_id)) return;
      seen.current.add(entry.hcom_id);
      setState((s) => ({ ...s, entries: [...s.entries, entry] }));
    });

    return () => {
      cancelled = true;
      es?.close();
    };
    // Re-seed + re-scope the live filter whenever run or either facet changes.
  }, [run, filter.task, filter.kind]);

  return state;
}

/** One task's full hcom trace (`GET /tasks/:id/hcom`). Keyed under `["hcom", …]`
 *  so the global SSE listener's invalidation refreshes it live. */
export function useTaskHcomLog(taskId: string | null) {
  return useQuery({
    queryKey: ["hcom", "task", taskId],
    queryFn: ({ signal }) => getTaskHcomLog(taskId!, signal),
    enabled: !!taskId,
  });
}

/** The deep transcript for a task — fetched on demand (it is large and live, not
 *  stored). `enabled` gates it behind an explicit "Load full transcript" click. */
export function useTaskTranscript(taskId: string | null, enabled: boolean) {
  return useQuery({
    queryKey: ["hcom", "transcript", taskId],
    queryFn: ({ signal }) => getTaskTranscript(taskId!, signal),
    enabled: !!taskId && enabled,
    staleTime: Infinity,
    retry: false,
  });
}

/** The agent's live transcript narration — the Claude-Code-style "what I'm doing"
 *  reasoning stream. Polls while `live` (the task is running) so the activity feed
 *  shows the agent's prose as it works, not just coarse tool ticks. Re-runs hcom
 *  each poll (the transcript isn't stored), so it always reflects the latest step.
 *  Stops polling once `live` is false (task finished/blocked); the last fetch
 *  remains for review. */
export function useLiveTranscript(taskId: string | null, live: boolean) {
  return useQuery({
    queryKey: ["hcom", "transcript", "live", taskId],
    queryFn: ({ signal }) => getTaskTranscript(taskId!, signal),
    enabled: !!taskId,
    // Poll only while the agent is active; a finished task keeps its final fetch.
    refetchInterval: live ? 2500 : false,
    retry: false,
  });
}
