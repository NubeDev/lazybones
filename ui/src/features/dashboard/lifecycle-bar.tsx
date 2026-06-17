import { Card, CardHeader, CardTitle, CardContent } from "@/components/ui/card";
import { STATUS_META } from "@/types/status-meta";
import { STATUSES, type Status, type Task } from "@/types/task";

/** A single stacked bar showing the share of tasks in each status, plus a
 *  per-status legend with counts. */
export function LifecycleBar({ tasks }: { tasks: Task[] }) {
  const counts = Object.fromEntries(
    STATUSES.map((s) => [s, tasks.filter((t) => t.status === s).length]),
  ) as Record<Status, number>;
  const total = tasks.length || 1;

  return (
    <Card>
      <CardHeader>
        <CardTitle>Lifecycle</CardTitle>
      </CardHeader>
      <CardContent className="space-y-4">
        <div className="flex h-2.5 w-full overflow-hidden rounded-full bg-muted">
          {STATUSES.map((s) => {
            const pct = (counts[s] / total) * 100;
            if (pct === 0) return null;
            return (
              <div
                key={s}
                style={{ width: `${pct}%`, backgroundColor: STATUS_META[s].color }}
                title={`${STATUS_META[s].label}: ${counts[s]}`}
              />
            );
          })}
        </div>

        <ul className="grid grid-cols-2 gap-2 sm:grid-cols-3">
          {STATUSES.map((s) => (
            <li key={s} className="flex items-center gap-2">
              <span
                className="size-2 shrink-0 rounded-full"
                style={{ backgroundColor: STATUS_META[s].color }}
              />
              <span className="text-xs text-muted-foreground">{STATUS_META[s].label}</span>
              <span className="ml-auto text-xs font-semibold tabular-nums">{counts[s]}</span>
            </li>
          ))}
        </ul>
      </CardContent>
    </Card>
  );
}
