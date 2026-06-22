import { FolderGit2 } from "lucide-react";
import { Card } from "@/components/ui/card";
import { Tooltip } from "@/components/ui/tooltip";
import { WorkflowStateBadge } from "@/components/ui/workflow-state-badge";
import { repoBasename } from "./repo-path";
import type { WorkflowSummary } from "@/types/workflow";

/** One workflow in the list: id/title, repo, derived-state pill, progress. */
export function WorkflowCard({
  workflow,
  onOpen,
}: {
  workflow: WorkflowSummary;
  onOpen: (id: string) => void;
}) {
  const { task_count, done_count } = workflow;
  const pct = task_count > 0 ? Math.round((done_count / task_count) * 100) : 0;

  return (
    <Card
      role="button"
      tabIndex={0}
      onClick={() => onOpen(workflow.id)}
      onKeyDown={(e) => {
        if (e.key === "Enter" || e.key === " ") {
          e.preventDefault();
          onOpen(workflow.id);
        }
      }}
      className="flex cursor-pointer flex-col gap-3 p-4 transition-colors hover:border-border-strong"
    >
      <div className="flex items-start justify-between gap-2">
        <div className="min-w-0">
          <h3 className="truncate text-sm font-semibold tracking-tight">
            {workflow.title}
          </h3>
          <span className="font-mono text-[11px] text-muted-foreground">
            {workflow.id}
          </span>
        </div>
        <WorkflowStateBadge state={workflow.state} />
      </div>

      <Tooltip label={workflow.workspace.repo} side="top">
        <span className="inline-flex w-fit max-w-full items-center gap-1.5 text-xs text-muted-foreground">
          <FolderGit2 className="size-3.5 shrink-0" />
          <span className="truncate font-mono">
            {repoBasename(workflow.workspace.repo)}
          </span>
        </span>
      </Tooltip>

      <div className="space-y-1">
        <div className="flex items-center justify-between text-[11px] text-muted-foreground">
          <span>
            {done_count} / {task_count} done
          </span>
          <span>{pct}%</span>
        </div>
        <div className="h-1.5 w-full overflow-hidden rounded-full bg-muted">
          <div
            className="h-full rounded-full bg-accent transition-all"
            style={{ width: `${pct}%` }}
          />
        </div>
      </div>
    </Card>
  );
}
