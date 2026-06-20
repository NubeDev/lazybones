import { useState } from "react";
import { FileText, ServerCrash } from "lucide-react";
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
import { useTaskHcomLog, useTaskTranscript } from "@/lib/hooks/use-hcom-log";
import { HcomLogRow } from "./hcom-log-row";

/** Per-task drill-in: this task's full hcom trace (`GET /tasks/:id/hcom`) plus a
 *  "Load full transcript" affordance that fetches the deep
 *  `GET /tasks/:id/transcript` view on demand. */
export function HcomTaskTrace({
  taskId,
  trigger,
}: {
  taskId: string;
  trigger: React.ReactNode;
}) {
  const [open, setOpen] = useState(false);
  const [wantTranscript, setWantTranscript] = useState(false);

  const { data: trace, isLoading, error } = useTaskHcomLog(open ? taskId : null);
  const transcript = useTaskTranscript(taskId, open && wantTranscript);

  return (
    <Dialog
      open={open}
      onOpenChange={(o) => {
        setOpen(o);
        if (!o) setWantTranscript(false);
      }}
    >
      <DialogTrigger asChild>{trigger}</DialogTrigger>
      <DialogContent title={taskId} description="hcom trace" className="max-w-3xl">
        <div className="mb-3 flex justify-end">
          <Button
            size="sm"
            variant="secondary"
            onClick={() => setWantTranscript(true)}
            disabled={wantTranscript && transcript.isFetching}
          >
            <FileText />
            {wantTranscript && transcript.isFetching
              ? "Loading…"
              : "Load full transcript"}
          </Button>
        </div>

        {error ? (
          <EmptyState
            icon={ServerCrash}
            title="Can't load trace"
            description={
              error instanceof ApiError ? error.message : "Unexpected error"
            }
          />
        ) : isLoading ? (
          <div className="space-y-2">
            {Array.from({ length: 5 }).map((_, i) => (
              <Skeleton key={i} className="h-7 w-full" />
            ))}
          </div>
        ) : !trace || trace.length === 0 ? (
          <EmptyState
            icon={FileText}
            title="No trace yet"
            description="This task hasn't produced any hcom events."
          />
        ) : (
          <ScrollArea className="max-h-[45vh] rounded-md border border-border">
            <ul className="divide-y divide-border px-3">
              {trace.map((e, i) => (
                <HcomLogRow key={`${e.hcom_id}-${i}`} entry={e} showTask={false} />
              ))}
            </ul>
          </ScrollArea>
        )}

        {wantTranscript && (
          <div className="mt-3 border-t border-border pt-3">
            {transcript.error ? (
              <p className="text-xs text-status-blocked">
                {transcript.error instanceof ApiError
                  ? transcript.error.message
                  : "Transcript unavailable (agent may be past hcom's retention)."}
              </p>
            ) : transcript.isFetching ? (
              <Skeleton className="h-32 w-full" />
            ) : transcript.data !== undefined ? (
              <ScrollArea className="max-h-[40vh] rounded-md border border-border bg-surface-2">
                <pre className="whitespace-pre-wrap break-words p-3 font-mono text-[11px] text-foreground">
                  {JSON.stringify(transcript.data, null, 2)}
                </pre>
              </ScrollArea>
            ) : null}
          </div>
        )}
      </DialogContent>
    </Dialog>
  );
}
