import { Coffee } from "lucide-react";
import { Card, CardHeader, CardTitle, CardContent } from "@/components/ui/card";
import { StatusBadge } from "@/components/ui/status-badge";
import { EmptyState } from "@/components/ui/empty-state";
import { relativeTime } from "@/lib/utils/platform";
import type { Task } from "@/types/task";

/** What's actively in flight: running + gating tasks with live heartbeat info. */
export function RunningPanel({
  tasks,
  onSelect,
}: {
  tasks: Task[];
  onSelect: (id: string) => void;
}) {
  return (
    <Card className="h-full">
      <CardHeader>
        <CardTitle>In flight</CardTitle>
      </CardHeader>
      <CardContent>
        {tasks.length === 0 ? (
          <EmptyState
            icon={Coffee}
            title="Nothing running"
            description="Idle. Promote ready tasks or let the loop pick them up."
            className="border-none py-8"
          />
        ) : (
          <ul className="space-y-2">
            {tasks.map((t) => (
              <li key={t.id}>
                <button
                  onClick={() => onSelect(t.id)}
                  className="flex w-full items-center gap-3 rounded-md border border-border bg-surface-2 px-3 py-2 text-left transition-colors hover:border-border-strong"
                >
                  <span className="font-mono text-xs font-semibold">{t.id}</span>
                  <span className="min-w-0 flex-1 truncate text-xs text-muted-foreground">
                    {t.title}
                  </span>
                  {t.heartbeat && (
                    <span className="shrink-0 text-[10px] text-muted-foreground">
                      {relativeTime(t.heartbeat)}
                    </span>
                  )}
                  <StatusBadge status={t.status} iconOnly />
                </button>
              </li>
            ))}
          </ul>
        )}
      </CardContent>
    </Card>
  );
}
