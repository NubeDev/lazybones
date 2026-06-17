import { Boxes, CheckCircle2, XCircle } from "lucide-react";
import { Card, CardHeader, CardTitle, CardDescription, CardContent } from "@/components/ui/card";
import { Skeleton } from "@/components/ui/skeleton";
import { useEngine } from "@/lib/hooks/use-agents";

/** Shows whether the hcom orchestration engine is available on the host. */
export function EngineCard() {
  const { data: engine, isLoading } = useEngine();

  return (
    <Card>
      <CardHeader>
        <CardTitle className="flex items-center gap-2">
          <Boxes className="size-4 text-accent" /> Orchestration engine
        </CardTitle>
        <CardDescription>
          lazybones is the queue + gate; the loop is hcom. The agents run inside
          hcom, so it must be installed on the host.
        </CardDescription>
      </CardHeader>
      <CardContent>
        {isLoading ? (
          <Skeleton className="h-10 w-full" />
        ) : (
          <div className="flex items-center gap-3 rounded-md border border-border bg-surface-2 px-3 py-2.5">
            {engine?.installed ? (
              <CheckCircle2 className="size-5 text-status-done" />
            ) : (
              <XCircle className="size-5 text-status-blocked" />
            )}
            <div className="min-w-0">
              <p className="text-sm font-medium">
                hcom{" "}
                {engine?.installed ? (
                  <span className="text-status-done">available</span>
                ) : (
                  <span className="text-status-blocked">not found</span>
                )}
              </p>
              <p className="truncate text-xs text-muted-foreground">
                {engine?.installed
                  ? (engine.version ?? "installed")
                  : (engine?.install_hint ?? "install hcom, then run `hcom status`")}
              </p>
            </div>
          </div>
        )}
      </CardContent>
    </Card>
  );
}
