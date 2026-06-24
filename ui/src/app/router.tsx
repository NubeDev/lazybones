import { useState } from "react";
import type { View } from "./navigation";
import { ExtRouteView, isExtRoute } from "@/lib/ext/slot-host";
import { DashboardPage } from "@/features/dashboard/dashboard-page";
import { TemplatesPage } from "@/features/templates/templates-page";
import { SkillsPage } from "@/features/skills/skills-page";
import { WorkflowsPage } from "@/features/workflows/workflows-page";
import { TasksPage } from "@/features/tasks/tasks-page";
import { RunsPage } from "@/features/runs/runs-page";
import { DocumentsPage } from "@/features/documents/documents-page";
import { BrandingPage } from "@/features/branding/branding-page";
import { ExtensionsPage } from "@/features/extensions/extensions-page";
import { SettingsPage } from "@/features/settings/settings-page";

/** A tiny in-memory router — no URLs to keep desktop + browser identical.
 *  The active view is a built-in [`View`] or an extension route id (`ext:…`),
 *  so the state is a plain string. Returns the active view and the navigator. */
export function useRouter() {
  const [view, setView] = useState<string>("dashboard");
  return { view, navigate: setView } as const;
}

export function ViewRenderer({
  view,
  onNavigate,
}: {
  view: string;
  onNavigate: (v: string) => void;
}) {
  // Extension-contributed pages render through the slot host (error-boundaried).
  if (isExtRoute(view)) return <ExtRouteView view={view} />;

  switch (view as View) {
    case "dashboard":
      return <DashboardPage onNavigate={onNavigate} />;
    case "templates":
      return <TemplatesPage />;
    case "skills":
      return <SkillsPage />;
    case "workflows":
      return <WorkflowsPage />;
    case "tasks":
      return <TasksPage />;
    case "runs":
      return <RunsPage />;
    case "documents":
      return <DocumentsPage />;
    case "branding":
      return <BrandingPage />;
    case "extensions":
      return <ExtensionsPage />;
    case "settings":
      return <SettingsPage />;
  }
}
