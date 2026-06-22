import { FileText, ServerCrash } from "lucide-react";
import { EmptyState } from "@/components/ui/empty-state";
import { Skeleton } from "@/components/ui/skeleton";
import { ScrollArea } from "@/components/ui/scroll-area";
import { ApiError } from "@/lib/api/client";
import { useTaskHcomLog } from "@/lib/hooks/use-hcom-log";
import { HcomLogRow } from "./hcom-log-row";

/** One task's hcom trace, rendered inline on the Tasks tab. The query behind
 *  `useTaskHcomLog` is `enabled` only when `active` is true, so the log is lazy
 *  loaded — nothing fetches until the user opens this task's Logs view. Keyed
 *  under `["hcom", …]` so the global SSE invalidation keeps it live while open. */
export function TaskLogsPanel({
  taskId,
  active,
}: {
  taskId: string;
  active: boolean;
}) {
  const { data: trace, isLoading, error } = useTaskHcomLog(active ? taskId : null);

  if (error) {
    return (
      <EmptyState
        icon={ServerCrash}
        title="Can't load logs"
        description={error instanceof ApiError ? error.message : "Unexpected error"}
      />
    );
  }

  if (isLoading) {
    return (
      <div className="space-y-2">
        {Array.from({ length: 5 }).map((_, i) => (
          <Skeleton key={i} className="h-7 w-full" />
        ))}
      </div>
    );
  }

  if (!trace || trace.length === 0) {
    return (
      <EmptyState
        icon={FileText}
        title="No logs yet"
        description="This task hasn't produced any hcom events."
      />
    );
  }

  return (
    <ScrollArea className="max-h-[45vh] rounded-md border border-border">
      <ul className="divide-y divide-border px-3">
        {trace.map((e, i) => (
          <HcomLogRow key={`${e.hcom_id}-${i}`} entry={e} showTask={false} />
        ))}
      </ul>
    </ScrollArea>
  );
}
