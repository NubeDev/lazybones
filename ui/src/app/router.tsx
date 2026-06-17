import { useState } from "react";
import type { View } from "./navigation";
import { DashboardPage } from "@/features/dashboard/dashboard-page";
import { TasksPage } from "@/features/tasks/tasks-page";
import { RunsPage } from "@/features/runs/runs-page";
import { SettingsPage } from "@/features/settings/settings-page";

/** A tiny in-memory router — no URLs to keep desktop + browser identical.
 *  Returns the active view and the renderer for the current page. */
export function useRouter() {
  const [view, setView] = useState<View>("dashboard");
  return { view, navigate: setView } as const;
}

export function ViewRenderer({
  view,
  onNavigate,
}: {
  view: View;
  onNavigate: (v: View) => void;
}) {
  switch (view) {
    case "dashboard":
      return <DashboardPage onNavigate={onNavigate} />;
    case "tasks":
      return <TasksPage />;
    case "runs":
      return <RunsPage />;
    case "settings":
      return <SettingsPage />;
  }
}
