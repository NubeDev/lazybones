import { useState } from "react";
import type { View } from "./navigation";
import { DashboardPage } from "@/features/dashboard/dashboard-page";
import { TemplatesPage } from "@/features/templates/templates-page";
import { SkillsPage } from "@/features/skills/skills-page";
import { WorkflowsPage } from "@/features/workflows/workflows-page";
import { TasksPage } from "@/features/tasks/tasks-page";
import { RunsPage } from "@/features/runs/runs-page";
import { DocumentsPage } from "@/features/documents/documents-page";
import { BrandingPage } from "@/features/branding/branding-page";
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
    case "settings":
      return <SettingsPage />;
  }
}
