import { useMemo, useState } from "react";
import { MessagesSquare, ServerCrash } from "lucide-react";
import { Card } from "@/components/ui/card";
import { ScrollArea } from "@/components/ui/scroll-area";
import { EmptyState } from "@/components/ui/empty-state";
import { Skeleton } from "@/components/ui/skeleton";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils/cn";
import { useHcomLogFeed, type HcomFilter } from "@/lib/hooks/use-hcom-log";
import type { HcomLogKind } from "@/types/event";
import type { Task } from "@/types/task";
import { HcomLogRow } from "./hcom-log-row";
import { HcomTaskTrace } from "./hcom-task-trace";

const KINDS: HcomLogKind[] = ["message", "status", "life"];

/** A pill toggle in the filter bar. */
function Pill({
  active,
  onClick,
  children,
}: {
  active: boolean;
  onClick: () => void;
  children: React.ReactNode;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      className={cn(
        "rounded-full border px-2.5 py-0.5 text-[11px] font-medium transition-colors",
        active
          ? "border-accent/40 bg-accent-soft/50 text-accent"
          : "border-border bg-surface-2 text-muted-foreground hover:text-foreground",
      )}
    >
      {children}
    </button>
  );
}

/** The Logs tab. Seeds the run's hcom log from `GET /runs/:id/hcom` and appends
 *  live `hcom_log` SSE entries (re-seeding from the durable log on reconnect).
 *  Filtering by task/kind re-queries the server for correctness and gates which
 *  live entries get appended. Rendered newest-activity-first, grouped by
 *  agent/task; each task header drills into its full trace + deep transcript. */
export function WorkflowHcomLog({ tasks }: { tasks: Task[] }) {
  // The shared run label every task in this workflow logs under.
  const runLabel = tasks[0]?.run ?? null;
  const [filter, setFilter] = useState<HcomFilter>({ task: null, kind: null });

  const { entries, isLoading, error, connected } = useHcomLogFeed(runLabel, filter);

  // Newest activity first.
  const ordered = useMemo(() => [...entries].reverse(), [entries]);

  if (!runLabel)
    return (
      <EmptyState
        icon={MessagesSquare}
        title="No tasks yet"
        description="Add tasks and start the workflow — agent logs show up here."
      />
    );

  const setKind = (k: HcomLogKind | null) =>
    setFilter((f) => ({ ...f, kind: f.kind === k ? null : k }));
  const setTask = (t: string | null) => setFilter((f) => ({ ...f, task: t }));

  return (
    <Card className="overflow-hidden">
      {/* Filter bar */}
      <div className="flex flex-wrap items-center gap-x-4 gap-y-2 border-b border-border px-4 py-2.5">
        <div className="flex items-center gap-1.5">
          <span className="text-[11px] uppercase tracking-wider text-muted-foreground">
            Task
          </span>
          <Pill active={filter.task === null} onClick={() => setTask(null)}>
            all
          </Pill>
          {tasks.map((t) => (
            <Pill
              key={t.id}
              active={filter.task === t.id}
              onClick={() => setTask(filter.task === t.id ? null : t.id)}
            >
              {t.id}
            </Pill>
          ))}
        </div>
        <div className="flex items-center gap-1.5">
          <span className="text-[11px] uppercase tracking-wider text-muted-foreground">
            Kind
          </span>
          <Pill active={filter.kind === null} onClick={() => setKind(null)}>
            all
          </Pill>
          {KINDS.map((k) => (
            <Pill key={k} active={filter.kind === k} onClick={() => setKind(k)}>
              {k}
            </Pill>
          ))}
        </div>
        {filter.task && (
          <HcomTaskTrace
            taskId={filter.task}
            trigger={
              <Button size="sm" variant="secondary" className="ml-auto">
                Open trace
              </Button>
            }
          />
        )}
      </div>

      {error && !connected ? (
        <div className="p-4">
          <EmptyState icon={ServerCrash} title="Can't load logs" />
        </div>
      ) : isLoading && entries.length === 0 ? (
        <div className="space-y-2 p-4">
          {Array.from({ length: 6 }).map((_, i) => (
            <Skeleton key={i} className="h-7 w-full" />
          ))}
        </div>
      ) : ordered.length === 0 ? (
        <div className="p-4">
          <EmptyState
            icon={MessagesSquare}
            title="No logs yet"
            description="Start the workflow — what each agent says streams in here."
          />
        </div>
      ) : (
        <ScrollArea className="max-h-[55vh]">
          <ul className="divide-y divide-border px-4">
            {ordered.map((e, i) => (
              <HcomLogRow key={`${e.hcom_id}-${i}`} entry={e} />
            ))}
          </ul>
        </ScrollArea>
      )}
    </Card>
  );
}
