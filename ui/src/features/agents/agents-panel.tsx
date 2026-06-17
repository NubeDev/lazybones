import { ServerCrash } from "lucide-react";
import { Card, CardHeader, CardTitle, CardDescription, CardContent } from "@/components/ui/card";
import { Skeleton } from "@/components/ui/skeleton";
import { EmptyState } from "@/components/ui/empty-state";
import { EngineCard } from "./engine-card";
import { AgentRow } from "./agent-row";
import { useAgents, useSecrets } from "@/lib/hooks/use-agents";

/** Settings section: orchestration engine status + agent CLI credential setup.
 *  Keys are sealed by the daemon (AES-GCM at rest) — the UI only ever sends a
 *  value or sees `…last4`. */
export function AgentsPanel() {
  const { data: agents, isLoading, error } = useAgents();
  const { data: secrets } = useSecrets();
  const secretByTool = new Map((secrets ?? []).map((s) => [s.tool, s]));

  return (
    <div className="space-y-4">
      <EngineCard />

      <Card>
        <CardHeader>
          <CardTitle>Agent credentials</CardTitle>
          <CardDescription>
            Each task runs an agent CLI. Add the API key for the tools you use —
            stored encrypted in the daemon and exported to agents at spawn. Values
            are write-only; only a <code>…last4</code> hint is ever shown.
          </CardDescription>
        </CardHeader>
        <CardContent className="space-y-3">
          {error ? (
            <EmptyState
              icon={ServerCrash}
              title="Can't load agents"
              description="Check the daemon connection + loop token above."
              className="border-none py-8"
            />
          ) : isLoading || !agents ? (
            <div className="space-y-3">
              {Array.from({ length: 3 }).map((_, i) => (
                <Skeleton key={i} className="h-32 w-full" />
              ))}
            </div>
          ) : (
            agents.map((a) => (
              <AgentRow key={a.tool} agent={a} secret={secretByTool.get(a.tool)} />
            ))
          )}
        </CardContent>
      </Card>
    </div>
  );
}
