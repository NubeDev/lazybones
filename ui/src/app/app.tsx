import { useEffect, useState } from "react";
import { Bot } from "lucide-react";
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

/** The root shell: providers + the sidebar/content split + the Lazybones Agent. */
export function App() {
  return (
    <ThemeProvider>
      <QueryProvider>
        <TooltipProvider delayDuration={300}>
          <AgentContextProvider>
            <Shell />
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
  // Load the operator's saved preferences (mirrors the timezone into
  // localStorage so the synchronous date formatters use it app-wide).
  usePreferences();

  // Keep the agent grounded in which page the operator is on (scope §7).
  useEffect(() => {
    setView(view);
  }, [view, setView]);

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
