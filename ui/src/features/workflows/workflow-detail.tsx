import { useMemo, useState } from "react";
import {
  ArrowLeft,
  FolderGit2,
  GitBranch,
  ServerCrash,
  AlertTriangle,
  PlayCircle,
  Clock,
  Bot,
  Cpu,
  Gauge,
} from "lucide-react";
import { Topbar } from "@/components/layout/topbar";
import { Button } from "@/components/ui/button";
import { Tooltip } from "@/components/ui/tooltip";
import { EmptyState } from "@/components/ui/empty-state";
import { Skeleton } from "@/components/ui/skeleton";
import { WorkflowStateBadge } from "@/components/ui/workflow-state-badge";
import { Tabs, TabsList, TabsTrigger, TabsContent } from "@/components/ui/tabs";
import { ApiError } from "@/lib/api/client";
import {
  useWorkflow,
  useWorkflowTasks,
  useResumeWorkflow,
} from "@/lib/hooks/use-workflows";
import { WORKTREE_MODES } from "@/features/tasks/worktree-mode";
import { TaskBoard } from "@/features/tasks/task-board";
import { TaskPage } from "@/features/tasks/detail/task-page";
import { duration, shortTime } from "@/lib/utils/platform";
import { useSetAgentContext } from "@/features/agent/agent-context";
import { repoBasename } from "./repo-path";
import { PlanGraphView } from "./plan-graph-view";
import { WorkflowTasks } from "./workflow-tasks";
import { WorkflowEvents } from "./workflow-events";
import { WorkflowHcomLog } from "./workflow-hcom-log";
import { WorkflowFollowUps } from "./workflow-follow-ups";
import { WorkflowControls } from "./workflow-controls";
import { AddTaskDialog } from "./add-task-dialog";
import { ReorderDialog } from "./reorder-dialog";
import { EditWorkflowDialog } from "./edit-workflow-dialog";
import { WorkflowIssues } from "./workflow-issues";
import { WorkflowPrs } from "./workflow-prs";
import { WorkflowGit } from "./workflow-git";
import { WorkflowFiles } from "./workflow-files";

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
  const { data: workflowTasks } = useWorkflowTasks(id);
  const resume = useResumeWorkflow();

  // Ground the Lazybones Agent in this workflow while the panel is open (scope §7).
  useSetAgentContext({
    workflow_id: id,
    run_id: id,
    repo: wf?.workspace.repo,
    base_branch: wf?.workspace.base_branch ?? undefined,
    task_id: undefined,
  });
  const [tab, setTab] = useState("plan");
  // The task opened in the shared detail panel (from Plan/Board clicks), or null.
  const [selectedId, setSelectedId] = useState<string | null>(null);

  // The server already scopes these to this workflow's `run_id`; the filter is a
  // defensive second layer so a foreign task can never render here even if the
  // endpoint regresses.
  const runTasks = useMemo(
    () => (workflowTasks ?? []).filter((t) => t.run_id === id),
    [workflowTasks, id],
  );
  const blockedCount = useMemo(
    () => runTasks.filter((t) => t.status === "blocked").length,
    [runTasks],
  );
  // Lookup for the detail panel, so it can render instantly from cache while it
  // refetches and resolve deps/onSelect to sibling tasks.
  const byId = useMemo(
    () => new Map(runTasks.map((t) => [t.id, t])),
    [runTasks],
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

  // Clicking a Plan node / Board card opens the task as a full page over the
  // workflow; Back returns to the workflow detail.
  if (selectedId) {
    return (
      <TaskPage
        id={selectedId}
        byId={byId}
        onBack={() => setSelectedId(null)}
        onSelect={setSelectedId}
        onDeleted={() => setSelectedId(null)}
      />
    );
  }

  return (
    <div className="flex h-full min-w-0 flex-col">
      <Topbar
        title={wf.title}
        subtitle={`workflow ${wf.id}`}
        actions={
          <div className="flex items-center gap-2">
            <WorkflowControls workflow={wf} onDeleted={onBack} />
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

          {/* The workflow's default agent triple — what its tasks inherit unless
              a task pins its own (most-specific-wins at execution time). Click to
              edit the workflow's agent + git defaults. */}
          <EditWorkflowDialog
            workflow={wf}
            trigger={
              <button
                type="button"
                title="Edit the workflow's agent + git defaults"
                className="inline-flex items-center gap-3 rounded-md px-1.5 py-0.5 text-xs text-muted-foreground transition-colors hover:bg-surface-2 hover:text-foreground"
              >
                <span className="inline-flex items-center gap-1.5">
                  <Bot className="size-3.5" />
                  <span className="font-mono">{wf.workspace.tool ?? "default"}</span>
                </span>
                <span className="inline-flex items-center gap-1.5">
                  <Cpu className="size-3.5" />
                  <span className="font-mono">{wf.workspace.model ?? "default"}</span>
                </span>
                <span className="inline-flex items-center gap-1.5">
                  <Gauge className="size-3.5" />
                  <span className="font-mono">{wf.workspace.effort ?? "default"}</span>
                </span>
              </button>
            }
          />

          {/* Lifecycle timing: when the run started, when it settled (or its
              latest failure), and the elapsed time. Only shows once started; the
              duration runs to the finish stamp, or live to now while in flight. */}
          {wf.started_at && (
            <Tooltip
              label={
                wf.finished_at
                  ? `Started ${shortTime(wf.started_at)} · finished ${shortTime(wf.finished_at)}`
                  : `Started ${shortTime(wf.started_at)}`
              }
              side="bottom"
            >
              <span className="inline-flex items-center gap-1.5 text-xs text-muted-foreground">
                <Clock className="size-3.5" />
                <span>{duration(wf.started_at, wf.finished_at)}</span>
                {!wf.finished_at && (
                  <span className="text-muted-foreground/70">· running</span>
                )}
                {wf.failed_at && !wf.finished_at && (
                  <span className="text-status-blocked">
                    · failed {shortTime(wf.failed_at)}
                  </span>
                )}
              </span>
            </Tooltip>
          )}

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

        {/* Stopped banner: the workflow is paused (lifecycle=stopped). The
            scheduler promotes/claims nothing and task-level retries are refused
            until it resumes — so surface a prominent Resume right here. Stopped is
            reversible, never a tombstone (delete is the archive path). */}
        {wf.state === "stopped" && (
          <div className="mb-4 flex flex-wrap items-center gap-3 rounded-lg border border-status-pending/40 bg-status-pending/10 p-4">
            <PlayCircle className="size-4 shrink-0 text-status-pending" />
            <div className="min-w-0 flex-1">
              <p className="text-sm font-medium text-foreground">
                Workflow stopped
              </p>
              <p className="text-xs text-muted-foreground">
                Paused by an operator — the scheduler is promoting nothing and
                task retries are refused. Resume to continue from where it left
                off.
              </p>
            </div>
            {resume.error instanceof ApiError && (
              <span className="text-[11px] text-status-blocked">
                {resume.error.message}
              </span>
            )}
            <Button
              size="sm"
              className="shrink-0"
              disabled={resume.isPending}
              onClick={() => resume.mutate(wf.id)}
            >
              <PlayCircle /> Resume
            </Button>
          </div>
        )}

        {/* Needs-attention banner: the workflow stalled on one or more blocked
            tasks. Surface Resume right here (it resets only the blocked tasks to
            pending so the run continues from where it broke) so the operator
            doesn't have to hunt for it in the header controls. */}
        {blockedCount > 0 && (
          <div className="mb-4 flex flex-wrap items-center gap-3 rounded-lg border border-status-blocked/30 bg-status-blocked/10 p-4">
            <AlertTriangle className="size-4 shrink-0 text-status-blocked" />
            <div className="min-w-0 flex-1">
              <p className="text-sm font-medium text-foreground">
                {blockedCount} task{blockedCount > 1 ? "s" : ""} blocked
              </p>
              <p className="text-xs text-muted-foreground">
                Resume retries just the blocked tasks — done and in-flight work is
                kept. Or retry tasks individually from the board or Tasks tab.
              </p>
            </div>
            {resume.error instanceof ApiError && (
              <span className="text-[11px] text-status-blocked">
                {resume.error.message}
              </span>
            )}
            <Button
              size="sm"
              className="shrink-0"
              disabled={resume.isPending}
              onClick={() => resume.mutate(wf.id)}
            >
              <PlayCircle /> Resume
            </Button>
          </div>
        )}

        <Tabs value={tab} onValueChange={setTab}>
          <div className="mb-4 flex items-center justify-between gap-2">
            <TabsList>
              <TabsTrigger value="plan">Plan</TabsTrigger>
              <TabsTrigger value="tasks">Tasks</TabsTrigger>
              <TabsTrigger value="board">Board</TabsTrigger>
              <TabsTrigger value="files">Files</TabsTrigger>
              <TabsTrigger value="git">Git</TabsTrigger>
              <TabsTrigger value="events">Events</TabsTrigger>
              <TabsTrigger value="logs">Logs</TabsTrigger>
              <TabsTrigger value="follow-ups">Follow-ups</TabsTrigger>
              <TabsTrigger value="issues">Issues</TabsTrigger>
              <TabsTrigger value="prs">Pull requests</TabsTrigger>
            </TabsList>
            {/* "Add task" belongs to the Tasks tab — show it only there so each
                tab owns its own primary action (Issues has its own "New issue").
                Reorder lives beside it: it rewrites deps to set execution order. */}
            {tab === "tasks" && (
              <div className="flex items-center gap-2">
                {runTasks.length > 1 && <ReorderDialog tasks={runTasks} />}
                <AddTaskDialog workflow={wf} existingTasks={wf.task_ids} />
              </div>
            )}
          </div>

          <TabsContent value="plan">
            <PlanGraphView
              tasks={runTasks}
              defaults={wf.workspace}
              onSelect={setSelectedId}
            />
          </TabsContent>

          <TabsContent value="tasks">
            <WorkflowTasks tasks={runTasks} />
          </TabsContent>

          <TabsContent value="board">
            <div className="h-[60vh]">
              <TaskBoard
                tasks={runTasks}
                isLoading={isLoading}
                error={null}
                selectedId={selectedId}
                onSelect={setSelectedId}
              />
            </div>
          </TabsContent>

          <TabsContent value="files">
            <WorkflowFiles
              dir={wf.workspace.repo}
              base={wf.workspace.base_branch ?? null}
            />
          </TabsContent>

          <TabsContent value="git">
            <WorkflowGit
              dir={wf.workspace.repo}
              base={wf.workspace.base_branch ?? null}
              tasks={runTasks}
            />
          </TabsContent>

          <TabsContent value="events">
            <WorkflowEvents tasks={runTasks} />
          </TabsContent>

          <TabsContent value="logs">
            <WorkflowHcomLog tasks={runTasks} />
          </TabsContent>

          <TabsContent value="follow-ups">
            <WorkflowFollowUps run={wf.id} />
          </TabsContent>

          <TabsContent value="issues">
            <WorkflowIssues dir={wf.workspace.repo} />
          </TabsContent>

          <TabsContent value="prs">
            <WorkflowPrs
              dir={wf.workspace.repo}
              base={wf.workspace.base_branch ?? null}
              tasks={runTasks}
            />
          </TabsContent>
        </Tabs>
      </div>

    </div>
  );
}
