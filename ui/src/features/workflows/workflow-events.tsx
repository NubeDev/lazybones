import { useMemo } from "react";
import { History, ServerCrash } from "lucide-react";
import { Card } from "@/components/ui/card";
import { ScrollArea } from "@/components/ui/scroll-area";
import { EmptyState } from "@/components/ui/empty-state";
import { Skeleton } from "@/components/ui/skeleton";
import { EventRow } from "@/features/runs/event-row";
import { useRunHistory } from "@/lib/hooks/use-run-history";
import type { Task } from "@/types/task";

/** The Events tab. The durable event log is keyed by the event-grouping `run`
 *  label (shared across workflows on the same daemon), not the workflow id — so
 *  we fetch the log under that label and filter to this workflow's task ids
 *  (keyed off `run_id`, never the dotted name). Live via the shared SSE stream,
 *  which invalidates the `run` query on every transition. */
export function WorkflowEvents({ tasks }: { tasks: Task[] }) {
  // The shared run label every task in this workflow logs under.
  const runLabel = tasks[0]?.run ?? null;
  const taskIds = useMemo(() => new Set(tasks.map((t) => t.id)), [tasks]);

  const { data: events, isLoading, error } = useRunHistory(runLabel);

  const ordered = useMemo(() => {
    if (!events) return undefined;
    return [...events].filter((e) => taskIds.has(e.task)).reverse();
  }, [events, taskIds]);

  if (!runLabel)
    return (
      <EmptyState
        icon={History}
        title="No tasks yet"
        description="Add tasks and start the workflow — transitions show up here."
      />
    );
  if (error) return <EmptyState icon={ServerCrash} title="Can't load events" />;
  if (isLoading && !events)
    return (
      <div className="space-y-2">
        {Array.from({ length: 6 }).map((_, i) => (
          <Skeleton key={i} className="h-8 w-full" />
        ))}
      </div>
    );
  if (!ordered || ordered.length === 0)
    return (
      <EmptyState
        icon={History}
        title="No transitions yet"
        description="Start the workflow — every status change for its tasks shows up here."
      />
    );

  return (
    <Card className="overflow-hidden">
      <div className="flex items-center gap-3 border-b border-border px-4 py-2.5 text-[11px] font-semibold uppercase tracking-wider text-muted-foreground">
        <span className="w-32">When</span>
        <span className="w-24">Task</span>
        <span>Transition</span>
        <span className="ml-auto">Actor</span>
      </div>
      <ScrollArea className="max-h-[55vh]">
        <ul className="divide-y divide-border px-4">
          {ordered.map((e, i) => (
            <EventRow key={`${e.task}-${e.at}-${i}`} event={e} />
          ))}
        </ul>
      </ScrollArea>
    </Card>
  );
}
