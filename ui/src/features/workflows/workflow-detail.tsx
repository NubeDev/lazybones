import { useMemo } from "react";
import { ArrowLeft, FolderGit2, GitBranch, ServerCrash } from "lucide-react";
import { Topbar } from "@/components/layout/topbar";
import { Button } from "@/components/ui/button";
import { Tooltip } from "@/components/ui/tooltip";
import { EmptyState } from "@/components/ui/empty-state";
import { Skeleton } from "@/components/ui/skeleton";
import { WorkflowStateBadge } from "@/components/ui/workflow-state-badge";
import { Tabs, TabsList, TabsTrigger, TabsContent } from "@/components/ui/tabs";
import { ApiError } from "@/lib/api/client";
import { useWorkflow } from "@/lib/hooks/use-workflows";
import { useTasks } from "@/lib/hooks/use-tasks";
import { WORKTREE_MODES } from "@/features/tasks/worktree-mode";
import { TaskBoard } from "@/features/tasks/task-board";
import { repoBasename } from "./repo-path";
import { PlanGraphView } from "./plan-graph-view";
import { WorkflowEvents } from "./workflow-events";
import { WorkflowControls } from "./workflow-controls";
import { AddTaskDialog } from "./add-task-dialog";
import { WorkflowIssues } from "./workflow-issues";
import { WorkflowGit } from "./workflow-git";

/** Workflow detail: workspace summary + derived state + progress, with Plan /
 *  Board / Events tabs. The board reuses the existing component, filtered to this
 *  workflow's `run_id` (Task.run). */
export function WorkflowDetail({
  id,
  onBack,
}: {
  id: string;
  onBack: () => void;
}) {
  const { data: wf, isLoading, error } = useWorkflow(id);
  const { data: allTasks } = useTasks();

  // Tasks belonging to this workflow, keyed off run_id (the real FK), never the
  // dotted board label.
  const runTasks = useMemo(
    () => (allTasks ?? []).filter((t) => t.run_id === id),
    [allTasks, id],
  );

  if (error) {
    const notFound = error instanceof ApiError && error.status === 404;
    return (
      <div className="flex h-full flex-col">
        <Topbar
          title={id}
          actions={
            <Button variant="ghost" size="sm" onClick={onBack}>
              <ArrowLeft /> Workflows
            </Button>
          }
        />
        <div className="flex-1 p-5">
          <EmptyState
            icon={ServerCrash}
            title={notFound ? "Workflow not found" : "Can't load workflow"}
            description={
              notFound
                ? "It may have been removed."
                : error instanceof ApiError
                  ? error.message
                  : "Unexpected error"
            }
          />
        </div>
      </div>
    );
  }

  if (isLoading && !wf) {
    return (
      <div className="flex h-full flex-col">
        <Topbar title={id} />
        <div className="flex-1 space-y-3 p-5">
          <Skeleton className="h-8 w-48" />
          <Skeleton className="h-24 w-full" />
        </div>
      </div>
    );
  }

  if (!wf) return null;

  const pct =
    wf.task_count > 0 ? Math.round((wf.done_count / wf.task_count) * 100) : 0;

  return (
    <div className="flex h-full min-w-0 flex-col">
      <Topbar
        title={wf.title}
        subtitle={`workflow ${wf.id}`}
        actions={
          <div className="flex items-center gap-2">
            <WorkflowControls workflow={wf} />
            <Button variant="ghost" size="sm" onClick={onBack}>
              <ArrowLeft /> Workflows
            </Button>
          </div>
        }
      />

      <div className="min-h-0 flex-1 overflow-auto p-5">
        {/* Workspace summary */}
        <div className="mb-4 flex flex-wrap items-center gap-x-6 gap-y-2 rounded-lg border border-border bg-surface p-4">
          <WorkflowStateBadge state={wf.state} />

          <Tooltip label={wf.workspace.repo} side="bottom">
            <span className="inline-flex items-center gap-1.5 text-xs text-muted-foreground">
              <FolderGit2 className="size-3.5" />
              <span className="font-mono">{repoBasename(wf.workspace.repo)}</span>
            </span>
          </Tooltip>

          <span className="inline-flex items-center gap-1.5 text-xs text-muted-foreground">
            <GitBranch className="size-3.5" />
            <span className="font-mono">{wf.workspace.base_branch ?? "default"}</span>
            {wf.workspace.branch_prefix && (
              <span className="font-mono text-muted-foreground/70">
                · {wf.workspace.branch_prefix}
              </span>
            )}
          </span>

          <span className="text-xs text-muted-foreground">
            {WORKTREE_MODES[wf.workspace.worktree_mode].label}
          </span>

          <div className="ml-auto flex items-center gap-2 text-xs text-muted-foreground">
            <span>
              {wf.done_count} / {wf.task_count} done
            </span>
            <div className="h-1.5 w-24 overflow-hidden rounded-full bg-muted">
              <div
                className="h-full rounded-full bg-accent transition-all"
                style={{ width: `${pct}%` }}
              />
            </div>
          </div>
        </div>

        <Tabs defaultValue="plan">
          <div className="mb-4 flex items-center justify-between gap-2">
            <TabsList>
              <TabsTrigger value="plan">Plan</TabsTrigger>
              <TabsTrigger value="board">Board</TabsTrigger>
              <TabsTrigger value="git">Git</TabsTrigger>
              <TabsTrigger value="events">Events</TabsTrigger>
              <TabsTrigger value="issues">Issues</TabsTrigger>
            </TabsList>
            <AddTaskDialog
              workflow={wf}
              existingTasks={wf.task_ids}
            />
          </div>

          <TabsContent value="plan">
            <PlanGraphView tasks={runTasks} />
          </TabsContent>

          <TabsContent value="board">
            <div className="h-[60vh]">
              <TaskBoard
                tasks={runTasks}
                isLoading={isLoading}
                error={null}
                selectedId={null}
                onSelect={() => {}}
              />
            </div>
          </TabsContent>

          <TabsContent value="git">
            <WorkflowGit dir={wf.workspace.repo} tasks={runTasks} />
          </TabsContent>

          <TabsContent value="events">
            <WorkflowEvents tasks={runTasks} />
          </TabsContent>

          <TabsContent value="issues">
            <WorkflowIssues dir={wf.workspace.repo} />
          </TabsContent>
        </Tabs>
      </div>
    </div>
  );
}
