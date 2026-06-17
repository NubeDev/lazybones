import { useMemo, useState } from "react";
import { Topbar } from "@/components/layout/topbar";
import { TaskBoard } from "./task-board";
import { TaskDetail } from "./detail/task-detail";
import { TasksToolbar } from "./tasks-toolbar";
import { useTasks, usePromote } from "@/lib/hooks/use-tasks";
import type { Task } from "@/types/task";

/** The Tasks view: a filterable board with a slide-in detail inspector. */
export function TasksPage() {
  const { data: tasks, isLoading, error, refetch, isFetching } = useTasks();
  const promote = usePromote();
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [query, setQuery] = useState("");

  const byId = useMemo(
    () => new Map((tasks ?? []).map((t) => [t.id, t])),
    [tasks],
  );

  const filtered = useMemo(() => {
    if (!tasks) return tasks;
    const q = query.trim().toLowerCase();
    if (!q) return tasks;
    return tasks.filter(
      (t: Task) =>
        t.id.toLowerCase().includes(q) ||
        t.title.toLowerCase().includes(q) ||
        (t.branch ?? "").toLowerCase().includes(q),
    );
  }, [tasks, query]);

  const subtitle = tasks
    ? `${tasks.length} task${tasks.length === 1 ? "" : "s"}${
        tasks[0] ? ` · run ${tasks[0].run}` : ""
      }`
    : "Loading…";

  return (
    <div className="flex h-full min-w-0 flex-col">
      <Topbar
        title="Tasks"
        subtitle={subtitle}
        actions={
          <TasksToolbar
            query={query}
            onQuery={setQuery}
            onPromote={() => promote.mutate()}
            onRefresh={() => refetch()}
            promoting={promote.isPending}
            refreshing={isFetching}
            tasks={tasks ?? []}
          />
        }
      />

      <div className="flex min-h-0 flex-1">
        <div className="min-w-0 flex-1 overflow-hidden p-5">
          <TaskBoard
            tasks={filtered}
            isLoading={isLoading}
            error={error}
            selectedId={selectedId}
            onSelect={setSelectedId}
          />
        </div>

        {selectedId && (
          <TaskDetail
            id={selectedId}
            byId={byId}
            onClose={() => setSelectedId(null)}
            onSelect={setSelectedId}
          />
        )}
      </div>
    </div>
  );
}
