import { useState } from "react";
import { X, Pencil, FileText, MessagesSquare, ScrollText } from "lucide-react";
import { Button } from "@/components/ui/button";
import { StatusBadge } from "@/components/ui/status-badge";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Skeleton } from "@/components/ui/skeleton";
import { Tabs, TabsList, TabsTrigger, TabsContent } from "@/components/ui/tabs";
import { BlockDialog } from "./block-dialog";
import { DeleteDialog } from "./delete-dialog";
import {
  SpecSection,
  DependenciesSection,
  DetailsSection,
  BlockedReason,
  RetrySection,
  ChatSection,
} from "./task-sections";
import { TaskDialog } from "../authoring/task-dialog";
import { StartDialog } from "../start-dialog";
import { TaskLogsPanel } from "@/features/workflows/task-logs-panel";
import { useTask } from "@/lib/hooks/use-tasks";
import { useSetAgentContext } from "@/features/agent/agent-context";
import type { Task } from "@/types/task";

/** The task inspector. Rather than one endless vertical scroll, the body is split
 *  across tabs — Overview (spec, deps, details, retry), Chat, and Logs — built
 *  from the reusable sections in `task-sections`, so the same pieces back both
 *  this panel and a full task page. */
export function TaskDetail({
  id,
  byId,
  onClose,
  onSelect,
}: {
  id: string;
  byId: Map<string, Task>;
  onClose: () => void;
  onSelect: (id: string) => void;
}) {
  // Prefer fresh detail, fall back to the list snapshot for instant paint.
  const { data, isLoading } = useTask(id);
  const task = data ?? byId.get(id);
  const [tab, setTab] = useState("overview");

  // Ground the Lazybones Agent in this task while the panel is open (scope §7).
  useSetAgentContext({
    task_id: id,
    run_id: task?.run_id ?? task?.run ?? undefined,
  });

  if (!task) {
    return (
      <Panel onClose={onClose}>
        <div className="space-y-3 p-5">
          <Skeleton className="h-6 w-32" />
          <Skeleton className="h-4 w-full" />
          <Skeleton className="h-40 w-full" />
        </div>
      </Panel>
    );
  }

  const terminal = task.status === "done" || task.status === "blocked";

  return (
    <Panel onClose={onClose}>
      <div className="flex items-start justify-between gap-3 border-b border-border p-5">
        <div className="min-w-0">
          <div className="flex items-center gap-2">
            <span className="font-mono text-sm font-bold">{task.id}</span>
            <StatusBadge status={task.status} />
          </div>
          <p className="mt-1 text-sm text-muted-foreground">{task.title}</p>
        </div>
        <Button variant="ghost" size="icon-sm" onClick={onClose} aria-label="Close">
          <X />
        </Button>
      </div>

      <Tabs value={tab} onValueChange={setTab} className="flex min-h-0 flex-1 flex-col">
        <div className="border-b border-border px-5 pt-3">
          <TabsList>
            <TabsTrigger value="overview">
              <FileText className="size-3.5" /> Overview
            </TabsTrigger>
            <TabsTrigger value="chat">
              <MessagesSquare className="size-3.5" /> Chat
            </TabsTrigger>
            <TabsTrigger value="logs">
              <ScrollText className="size-3.5" /> Logs
            </TabsTrigger>
          </TabsList>
        </div>

        {/* Overview — the bulk of the task's facts, as cards. */}
        <TabsContent value="overview" className="min-h-0 flex-1">
          <ScrollArea className="h-full">
            <div className="space-y-3 p-5">
              <SpecSection spec={task.spec} loading={isLoading && !data} />
              <BlockedReason task={task} />
              <DependenciesSection task={task} byId={byId} onSelect={onSelect} />
              <DetailsSection task={task} />
              <RetrySection task={task} />
            </div>
          </ScrollArea>
        </TabsContent>

        {/* Chat fills the panel so the composer sits at the bottom. */}
        <TabsContent value="chat" className="flex min-h-0 flex-1 flex-col p-5">
          <ChatSection task={task} />
        </TabsContent>

        {/* Logs lazy-load: the query is gated on this tab being active. */}
        <TabsContent value="logs" className="min-h-0 flex-1">
          <ScrollArea className="h-full">
            <div className="p-5">
              <TaskLogsPanel taskId={task.id} active={tab === "logs"} />
            </div>
          </ScrollArea>
        </TabsContent>
      </Tabs>

      <div className="flex items-center justify-between gap-2 border-t border-border p-4">
        <DeleteDialog taskId={task.id} onDeleted={onClose} />
        <div className="flex gap-2">
          {task.status === "pending" && <StartDialog task={task} byId={byId} />}
          <TaskDialog
            task={task}
            allTasks={[...byId.values()]}
            trigger={
              <Button variant="secondary" size="sm">
                <Pencil /> Edit
              </Button>
            }
          />
          {!terminal && <BlockDialog taskId={task.id} />}
        </div>
      </div>
    </Panel>
  );
}

function Panel({ children, onClose }: { children: React.ReactNode; onClose: () => void }) {
  return (
    <>
      {/* Mobile scrim */}
      <div className="fixed inset-0 z-20 bg-black/40 lg:hidden" onClick={onClose} />
      <div className="fixed inset-y-0 right-0 z-30 flex w-full max-w-lg flex-col border-l border-border bg-surface shadow-2xl animate-fade-up lg:static lg:z-0 lg:shadow-none">
        {children}
      </div>
    </>
  );
}
