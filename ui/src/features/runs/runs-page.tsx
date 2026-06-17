import { History, ServerCrash } from "lucide-react";
import { Topbar } from "@/components/layout/topbar";
import { EventRow } from "./event-row";
import { Card } from "@/components/ui/card";
import { ScrollArea } from "@/components/ui/scroll-area";
import { EmptyState } from "@/components/ui/empty-state";
import { Skeleton } from "@/components/ui/skeleton";
import { useTasks } from "@/lib/hooks/use-tasks";
import { useRunHistory } from "@/lib/hooks/use-run-history";

/** The Run history view: the full transition log for the active run, newest
 *  first. The run id is taken from the loaded tasks (one run per workfile). */
export function RunsPage() {
  const { data: tasks } = useTasks();
  const run = tasks?.[0]?.run ?? null;
  const { data: events, isLoading, error } = useRunHistory(run);

  const ordered = events ? [...events].reverse() : undefined;

  return (
    <div className="flex h-full flex-col">
      <Topbar
        title="Run history"
        subtitle={run ? `run ${run}` : "No active run"}
      />
      <div className="flex-1 overflow-hidden p-5">
        {!run ? (
          <EmptyState
            icon={History}
            title="No run loaded"
            description="Import a workfile to start a run; its transitions appear here as queryable rows."
          />
        ) : error ? (
          <EmptyState icon={ServerCrash} title="Can't load history" />
        ) : isLoading ? (
          <div className="space-y-2">
            {Array.from({ length: 8 }).map((_, i) => (
              <Skeleton key={i} className="h-8 w-full" />
            ))}
          </div>
        ) : !ordered || ordered.length === 0 ? (
          <EmptyState
            icon={History}
            title="No transitions yet"
            description="Every status change is recorded here as a row — promote or run tasks to populate it."
          />
        ) : (
          <Card className="h-full overflow-hidden">
            <div className="flex items-center gap-3 border-b border-border px-4 py-2.5 text-[11px] font-semibold uppercase tracking-wider text-muted-foreground">
              <span className="w-32">When</span>
              <span className="w-24">Task</span>
              <span>Transition</span>
              <span className="ml-auto">Actor</span>
            </div>
            <ScrollArea className="h-[calc(100%-2.6rem)]">
              <ul className="divide-y divide-border px-4">
                {ordered.map((e, i) => (
                  <EventRow key={`${e.task}-${e.at}-${i}`} event={e} />
                ))}
              </ul>
            </ScrollArea>
          </Card>
        )}
      </div>
    </div>
  );
}
