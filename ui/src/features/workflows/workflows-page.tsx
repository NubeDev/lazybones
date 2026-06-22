import { useState } from "react";
import { Workflow as WorkflowIcon, ServerCrash } from "lucide-react";
import { Topbar } from "@/components/layout/topbar";
import { EmptyState } from "@/components/ui/empty-state";
import { Skeleton } from "@/components/ui/skeleton";
import { ApiError } from "@/lib/api/client";
import { useWorkflows } from "@/lib/hooks/use-workflows";
import { WorkflowCard } from "./workflow-card";
import { WorkflowDialog } from "./workflow-dialog";
import { WorkflowDetail } from "./workflow-detail";

/** The Workflows view: a list of workflow cards, or a single workflow's detail.
 *  Selection lives here (no URL router) so list ↔ detail stays in-memory. */
export function WorkflowsPage() {
  const [selected, setSelected] = useState<string | null>(null);

  if (selected) {
    return <WorkflowDetail id={selected} onBack={() => setSelected(null)} />;
  }

  return <WorkflowsList onOpen={setSelected} />;
}

function WorkflowsList({ onOpen }: { onOpen: (id: string) => void }) {
  const { data: workflows, isLoading, error } = useWorkflows();

  const subtitle = workflows
    ? `${workflows.length} workflow${workflows.length === 1 ? "" : "s"}`
    : "Loading…";

  return (
    <div className="flex h-full flex-col">
      <Topbar
        title="Workflows"
        subtitle={subtitle}
        actions={<WorkflowDialog onCreated={onOpen} />}
      />

      <div className="flex-1 overflow-auto p-5">
        {error ? (
          <EmptyState
            icon={ServerCrash}
            title="Can't load workflows"
            description={error instanceof ApiError ? error.message : "Unexpected error"}
          />
        ) : isLoading && !workflows ? (
          <div className="grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-3">
            {Array.from({ length: 3 }).map((_, i) => (
              <Skeleton key={i} className="h-36 w-full" />
            ))}
          </div>
        ) : !workflows || workflows.length === 0 ? (
          <EmptyState
            icon={WorkflowIcon}
            title="No workflows yet"
            description="Create a workflow bound to a repo, add tasks (some from templates), then start it — the scheduler claims ready tasks and runs them."
            action={<WorkflowDialog onCreated={onOpen} />}
          />
        ) : (
          <div className="grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-3">
            {workflows.map((wf) => (
              <WorkflowCard key={wf.id} workflow={wf} onOpen={onOpen} />
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
