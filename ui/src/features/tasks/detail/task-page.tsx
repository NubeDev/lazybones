import { useState } from "react";
import { ArrowLeft, Pencil, FileText, MessagesSquare, ScrollText } from "lucide-react";
import { Topbar } from "@/components/layout/topbar";
import { Button } from "@/components/ui/button";
import { StatusBadge } from "@/components/ui/status-badge";
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
import type { Task } from "@/types/task";

/** A full-width task page built from the same reusable sections as the inspector
 *  panel. A long task no longer scrolls forever in a narrow drawer: the facts sit
 *  in a roomy two-column layout (spec + details on the left, deps/retry on the
 *  right), with Chat and Logs behind their own tabs.
 *
 *  `byId` lets the page resolve dependency chips and edit-dialog siblings; pass an
 *  empty map for a standalone task. `onBack`/`onSelect` are optional navigation
 *  hooks. */
export function TaskPage({
  id,
  byId,
  onBack,
  onSelect,
  onDeleted,
}: {
  id: string;
  byId: Map<string, Task>;
  onBack?: () => void;
  onSelect?: (id: string) => void;
  onDeleted?: () => void;
}) {
  const { data, isLoading } = useTask(id);
  const task = data ?? byId.get(id);
  const [tab, setTab] = useState("overview");

  if (!task) {
    return (
      <div className="flex h-full flex-col">
        <Topbar title={id} subtitle="Task" />
        <div className="space-y-3 p-6">
          <Skeleton className="h-6 w-40" />
          <Skeleton className="h-4 w-full" />
          <Skeleton className="h-40 w-full" />
        </div>
      </div>
    );
  }

  const terminal = task.status === "done" || task.status === "blocked";

  return (
    <div className="flex h-full min-w-0 flex-col">
      <Topbar
        title={task.id}
        subtitle={task.title}
        actions={
          <div className="flex items-center gap-2">
            <StatusBadge status={task.status} />
            {onBack && (
              <Button variant="ghost" size="sm" onClick={onBack}>
                <ArrowLeft /> Back
              </Button>
            )}
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
            <DeleteDialog taskId={task.id} onDeleted={onDeleted ?? onBack ?? (() => {})} />
          </div>
        }
      />

      <Tabs value={tab} onValueChange={setTab} className="flex min-h-0 flex-1 flex-col">
        <div className="border-b border-border px-6 pt-3">
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

        {/* Overview — two columns on wide screens so a long task reads at a
            glance instead of one tall scroll. */}
        <TabsContent value="overview" className="min-h-0 flex-1 overflow-y-auto">
          <div className="mx-auto grid max-w-5xl gap-4 p-6 lg:grid-cols-2">
            <div className="space-y-4">
              <SpecSection spec={task.spec} loading={isLoading && !data} />
              <BlockedReason task={task} />
              <DetailsSection task={task} />
            </div>
            <div className="space-y-4">
              <DependenciesSection
                task={task}
                byId={byId}
                onSelect={onSelect ?? (() => {})}
              />
              <RetrySection task={task} />
            </div>
          </div>
        </TabsContent>

        <TabsContent value="chat" className="flex min-h-0 flex-1 flex-col p-6">
          <div className="mx-auto flex min-h-0 w-full max-w-3xl flex-1 flex-col">
            <ChatSection task={task} />
          </div>
        </TabsContent>

        <TabsContent value="logs" className="min-h-0 flex-1 overflow-y-auto">
          <div className="mx-auto max-w-5xl p-6">
            <TaskLogsPanel taskId={task.id} active={tab === "logs"} />
          </div>
        </TabsContent>
      </Tabs>
    </div>
  );
}
