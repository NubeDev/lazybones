import { cn } from "@/lib/utils/cn";
import { Tooltip } from "@/components/ui/tooltip";
import { useHealth } from "@/lib/hooks/use-health";
import { apiBase } from "@/lib/api/config";

/** A live dot reflecting lazybonesd reachability. */
export function ConnectionStatus() {
  const { data: online, isLoading } = useHealth();
  const state = isLoading ? "checking" : online ? "online" : "offline";

  const color = {
    online: "var(--color-status-done)",
    offline: "var(--color-status-blocked)",
    checking: "var(--color-status-pending)",
  }[state];

  const label = {
    online: "Connected to lazybonesd",
    offline: "lazybonesd unreachable",
    checking: "Checking connection…",
  }[state];

  return (
    <Tooltip label={`${label} · ${apiBase()}`}>
      <div className="flex items-center gap-2 rounded-md border border-border bg-surface-2 px-2.5 py-1.5 no-drag">
        <span
          className={cn("size-2 rounded-full", state === "online" && "animate-pulse-ring")}
          style={{ backgroundColor: color, color }}
        />
        <span className="text-xs font-medium capitalize text-muted-foreground">
          {state}
        </span>
      </div>
    </Tooltip>
  );
}
