import { useState } from "react";
import { ListTree, ServerCrash, Sparkles } from "lucide-react";
import {
  Dialog,
  DialogContent,
  DialogTrigger,
} from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
import { EmptyState } from "@/components/ui/empty-state";
import { Skeleton } from "@/components/ui/skeleton";
import { ScrollArea } from "@/components/ui/scroll-area";
import { ApiError } from "@/lib/api/client";
import { useTaskHcomLog, useLiveTranscript } from "@/lib/hooks/use-hcom-log";
import { useTask } from "@/lib/hooks/use-tasks";
import { HcomLogRow } from "./hcom-log-row";
import { TranscriptView } from "./transcript-view";

/** Per-task drill-in. Defaults to the **Activity** view — the agent's live
 *  narration stream (polled while it runs), which reads like Claude Code's
 *  "what I'm doing" feed. A secondary **Trace** view exposes the raw hcom event
 *  spine (message/status/life) for debugging. */
export function HcomTaskTrace({
  taskId,
  trigger,
}: {
  taskId: string;
  trigger: React.ReactNode;
}) {
  const [open, setOpen] = useState(false);
  const [view, setView] = useState<"activity" | "trace">("activity");

  const { data: task } = useTask(open ? taskId : null);
  const live = task?.status === "running";

  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogTrigger asChild>{trigger}</DialogTrigger>
      <DialogContent title={taskId} description="agent activity" className="max-w-3xl">
        <div className="mb-3 flex items-center gap-1">
          <Button
            size="sm"
            variant={view === "activity" ? "secondary" : "ghost"}
            onClick={() => setView("activity")}
          >
            <Sparkles />
            Activity
            {live && (
              <span className="ml-1 size-1.5 animate-pulse rounded-full bg-status-running" />
            )}
          </Button>
          <Button
            size="sm"
            variant={view === "trace" ? "secondary" : "ghost"}
            onClick={() => setView("trace")}
          >
            <ListTree />
            Trace
          </Button>
        </div>

        {view === "activity" ? (
          <ActivityPane taskId={open ? taskId : null} live={!!live} />
        ) : (
          <TracePane taskId={open ? taskId : null} />
        )}
      </DialogContent>
    </Dialog>
  );
}

/** The live narration feed: the agent's reasoning stream, polled while running. */
function ActivityPane({ taskId, live }: { taskId: string | null; live: boolean }) {
  const { data, isLoading, error, isFetching } = useLiveTranscript(taskId, live);

  if (error) {
    return (
      <EmptyState
        icon={ServerCrash}
        title="No live activity"
        description={
          error instanceof ApiError
            ? error.message
            : "The agent's transcript isn't available (it may be past hcom's retention)."
        }
      />
    );
  }
  if (isLoading) {
    return (
      <div className="space-y-2">
        {Array.from({ length: 6 }).map((_, i) => (
          <Skeleton key={i} className="h-8 w-full" />
        ))}
      </div>
    );
  }
  return (
    <div className="space-y-2">
      {live && (
        <p className="flex items-center gap-1.5 text-[11px] text-muted-foreground">
          <span className="size-1.5 animate-pulse rounded-full bg-status-running" />
          {isFetching ? "Updating…" : "Live — updates as the agent works"}
        </p>
      )}
      <TranscriptView data={data} />
    </div>
  );
}

/** The raw hcom event spine — message/status/life rows, for debugging. */
function TracePane({ taskId }: { taskId: string | null }) {
  const { data: trace, isLoading, error } = useTaskHcomLog(taskId);

  if (error) {
    return (
      <EmptyState
        icon={ServerCrash}
        title="Can't load trace"
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
        icon={ListTree}
        title="No trace yet"
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
