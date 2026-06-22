import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useState,
  type ReactNode,
} from "react";
import type { PageContext } from "@/types/page-context";

interface AgentContextValue {
  /** The page context attached to the next agent turn. */
  context: PageContext;
  /** Replace the whole context (the app shell sets `view`). */
  setView: (view: PageContext["view"]) => void;
  /** Merge in detail-panel ids (workflow/task/run/…). Returns a cleanup that
   *  removes exactly the keys this call set, so an unmounting panel reverts. */
  setContext: (partial: Partial<PageContext>) => () => void;
}

const Ctx = createContext<AgentContextValue | null>(null);

/** Holds the page context the Lazybones Agent is grounded in (scope §7).
 *  Mounted in the app shell; the shell sets `view`, detail panels layer ids on
 *  top via `useSetAgentContext`. */
export function AgentContextProvider({ children }: { children: ReactNode }) {
  const [view, setViewState] = useState<PageContext["view"]>(undefined);
  const [details, setDetails] = useState<Partial<PageContext>>({});

  const setView = useCallback((v: PageContext["view"]) => setViewState(v), []);

  const setContext = useCallback((partial: Partial<PageContext>) => {
    setDetails((prev) => ({ ...prev, ...partial }));
    const keys = Object.keys(partial) as (keyof PageContext)[];
    return () => {
      setDetails((prev) => {
        const next = { ...prev };
        for (const k of keys) delete next[k];
        return next;
      });
    };
  }, []);

  const value = useMemo<AgentContextValue>(
    () => ({ context: { view, ...details }, setView, setContext }),
    [view, details, setView, setContext],
  );

  return <Ctx.Provider value={value}>{children}</Ctx.Provider>;
}

/** The current page context + setters. Throws if used outside the provider. */
export function useAgentContext(): AgentContextValue {
  const ctx = useContext(Ctx);
  if (!ctx) throw new Error("useAgentContext must be used within AgentContextProvider");
  return ctx;
}

/** Layer detail-panel ids onto the agent's page context for the panel's
 *  lifetime. Pass the ids in scope (workflow_id/repo/task_id/run_id/…); they are
 *  removed when the panel unmounts. */
export function useSetAgentContext(partial: Partial<PageContext>): void {
  const { setContext } = useAgentContext();
  // Serialize so the effect only re-runs when a value actually changes.
  const key = JSON.stringify(partial);
  useEffect(() => {
    const cleanup = setContext(partial);
    return cleanup;
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [key, setContext]);
}
