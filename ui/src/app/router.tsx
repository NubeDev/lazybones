import { useState } from "react";
import { ShieldX } from "lucide-react";
import type { View } from "./navigation";
import { EmptyState } from "@/components/ui/empty-state";
import { useRole } from "@/lib/hooks/use-role";
import { DashboardPage } from "@/features/dashboard/dashboard-page";
import { ProjectsPage } from "@/features/projects/projects-page";
import { TeamDashboard } from "@/features/team/team-dashboard";
import { AdminPage } from "@/features/admin/admin-page";
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
  const { canSeeTeam, canSeeAdmin } = useRole();
  switch (view) {
    case "dashboard":
      return <DashboardPage onNavigate={onNavigate} />;
    case "projects":
      return <ProjectsPage />;
    case "team":
      return canSeeTeam ? <TeamDashboard /> : <Denied />;
    case "admin":
      return canSeeAdmin ? <AdminPage /> : <Denied />;
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

/** Shown when a principal reaches a role-scoped view they lack the authority for
 *  (the nav already hides it; this is the defense-in-depth fallback). */
function Denied() {
  return (
    <div className="flex h-full items-center justify-center p-8">
      <EmptyState
        icon={ShieldX}
        title="Not authorized"
        description="This section needs a higher role than your account holds. Ask an admin for access."
      />
    </div>
  );
}
