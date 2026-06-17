import { ArrowRight } from "lucide-react";
import { StatusDot } from "@/components/ui/status-badge";
import type { Task } from "@/types/task";

/** The task's dependency ids, resolved against the full task list so each shows
 *  its current status (so you can see what's blocking readiness). */
export function DepsList({
  deps,
  byId,
  onSelect,
}: {
  deps: string[];
  byId: Map<string, Task>;
  onSelect: (id: string) => void;
}) {
  if (deps.length === 0) {
    return <p className="text-xs text-muted-foreground">No dependencies.</p>;
  }
  return (
    <ul className="space-y-1">
      {deps.map((dep) => {
        const t = byId.get(dep);
        return (
          <li key={dep}>
            <button
              onClick={() => t && onSelect(dep)}
              disabled={!t}
              className="group flex w-full items-center gap-2 rounded-md border border-border bg-surface-2 px-2.5 py-1.5 text-left text-xs transition-colors hover:border-border-strong disabled:opacity-60"
            >
              {t ? <StatusDot status={t.status} /> : <span className="size-2" />}
              <span className="font-mono">{dep}</span>
              <span className="ml-auto truncate text-muted-foreground">
                {t?.title ?? "unknown"}
              </span>
              {t && (
                <ArrowRight className="size-3 shrink-0 text-muted-foreground opacity-0 transition-opacity group-hover:opacity-100" />
              )}
            </button>
          </li>
        );
      })}
    </ul>
  );
}
