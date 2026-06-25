import { useEffect, useState } from "react";
import { Bot } from "lucide-react";
import type { View } from "./navigation";
import { Sidebar } from "@/components/layout/sidebar";
import { Button } from "@/components/ui/button";
import { TooltipProvider } from "@/components/ui/tooltip";
import { ThemeProvider } from "@/lib/theme/theme-provider";
import { QueryProvider } from "./query-provider";
import { useRouter, ViewRenderer } from "./router";
import { useLiveStream } from "@/lib/hooks/use-live-stream";
import { usePreferences } from "@/lib/hooks/use-preferences";
import {
  AgentContextProvider,
  useAgentContext,
} from "@/features/agent/agent-context";
import { AgentPanel } from "@/features/agent/agent-panel";
import { useFrontendExtensions } from "@/lib/ext/use-extensions";
import { setExtRoute } from "@/lib/ext/context-store";
import { isExtRoute } from "@/lib/ext/slot-host";
import { ExtThemeBridge } from "@/lib/ext/theme";
import { ToastViewport } from "@/lib/ext/toast";
import { SyncBanner } from "@/components/layout/sync-banner";

/** The root shell: providers + the sidebar/content split + the Lazybones Agent.
 *  Also wires the frontend extension plane — the theme bridge that feeds remotes
 *  the live tokens, and the toast viewport that renders extension notifications. */
export function App() {
  return (
    <ThemeProvider>
      <ExtThemeBridge />
      <QueryProvider>
        <TooltipProvider delayDuration={300}>
          <AgentContextProvider>
            <Shell />
            <SyncBanner />
            <ToastViewport />
          </AgentContextProvider>
        </TooltipProvider>
      </QueryProvider>
    </ThemeProvider>
  );
}

function Shell() {
  const { view, navigate } = useRouter();
  const { setView } = useAgentContext();
  const [agentOpen, setAgentOpen] = useState(false);
  useLiveStream();
  // Install the SDK host services and load enabled frontend remotes on boot.
  useFrontendExtensions();
  // Load the operator's saved preferences (mirrors the timezone into
  // localStorage so the synchronous date formatters use it app-wide).
  usePreferences();

  // Keep the agent grounded in which page the operator is on (scope §7). An
  // extension route has no built-in `View`, so the agent sees `undefined` there.
  useEffect(() => {
    setView(isExtRoute(view) ? undefined : (view as View));
  }, [view, setView]);

  // Mirror the active view into the extension host context so remotes can react
  // to navigation (design §4.1 current-route context).
  useEffect(() => {
    setExtRoute({ view });
  }, [view]);

  return (
    <div className="flex h-screen w-screen overflow-hidden bg-background text-foreground">
      <Sidebar view={view} onNavigate={navigate} />
      <main className="relative min-w-0 flex-1">
        <ViewRenderer view={view} onNavigate={navigate} />
        {!agentOpen && (
          <Button
            size="sm"
            onClick={() => setAgentOpen(true)}
            title="Open the Lazybones Agent"
            className="absolute bottom-4 right-4 z-10 shadow-lg"
          >
            <Bot /> Agent
          </Button>
        )}
      </main>
      {agentOpen && <AgentPanel onClose={() => setAgentOpen(false)} />}
    </div>
  );
}
