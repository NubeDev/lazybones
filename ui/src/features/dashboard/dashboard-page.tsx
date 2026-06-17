import { Activity, CheckCircle2, Loader, AlertTriangle, ListTodo } from "lucide-react";
import { Topbar } from "@/components/layout/topbar";
import { StatCard } from "./stat-card";
import { LifecycleBar } from "./lifecycle-bar";
import { RunningPanel } from "./running-panel";
import { EmptyState } from "@/components/ui/empty-state";
import { Skeleton } from "@/components/ui/skeleton";
import { ServerCrash } from "lucide-react";
import { useTasks } from "@/lib/hooks/use-tasks";
import { progress } from "@/features/tasks/group-tasks";
import type { View } from "@/app/navigation";

/** The landing view: at-a-glance run health and what's in flight. */
export function DashboardPage({ onNavigate }: { onNavigate: (v: View) => void }) {
  const { data: tasks, isLoading, error } = useTasks();

  if (error) {
    return (
      <Shell>
        <EmptyState
          icon={ServerCrash}
          title="lazybonesd unreachable"
          description="Start the daemon (lazybonesd serve) and check the API address in Settings."
        />
      </Shell>
    );
  }

  if (isLoading || !tasks) {
    return (
      <Shell>
        <div className="grid grid-cols-2 gap-4 lg:grid-cols-4">
          {Array.from({ length: 4 }).map((_, i) => (
            <Skeleton key={i} className="h-24" />
          ))}
        </div>
        <Skeleton className="h-40" />
      </Shell>
    );
  }

  const running = tasks.filter((t) => t.status === "running" || t.status === "gating");
  const done = tasks.filter((t) => t.status === "done").length;
  const blocked = tasks.filter((t) => t.status === "blocked").length;
  const pct = Math.round(progress(tasks) * 100);

  return (
    <Shell>
      <div className="grid grid-cols-2 gap-4 lg:grid-cols-4">
        <StatCard
          label="Total tasks"
          value={tasks.length}
          icon={ListTodo}
          hint={tasks[0] ? `run ${tasks[0].run}` : undefined}
        />
        <StatCard
          label="In flight"
          value={running.length}
          icon={Loader}
          accent="var(--color-status-running)"
          hint="running + gating"
        />
        <StatCard
          label="Done"
          value={`${done}`}
          icon={CheckCircle2}
          accent="var(--color-status-done)"
          hint={`${pct}% complete`}
        />
        <StatCard
          label="Blocked"
          value={blocked}
          icon={AlertTriangle}
          accent="var(--color-status-blocked)"
          hint={blocked ? "needs triage" : "all clear"}
        />
      </div>

      <div className="grid gap-4 lg:grid-cols-5">
        <div className="lg:col-span-2">
          <LifecycleBar tasks={tasks} />
        </div>
        <div className="lg:col-span-3">
          <RunningPanel tasks={running} onSelect={() => onNavigate("tasks")} />
        </div>
      </div>
    </Shell>
  );
}

function Shell({ children }: { children: React.ReactNode }) {
  return (
    <div className="flex h-full flex-col">
      <Topbar title="Dashboard" subtitle="Run health at a glance" actions={<HeroPing />} />
      <div className="flex-1 space-y-4 overflow-y-auto p-5">{children}</div>
    </div>
  );
}

function HeroPing() {
  return (
    <span className="hidden items-center gap-1.5 text-xs text-muted-foreground sm:inline-flex">
      <Activity className="size-3.5 text-status-running" />
      live · polling
    </span>
  );
}
