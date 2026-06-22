import { useEffect, useState } from "react";
import {
  FolderGit2,
  FolderKanban,
  ServerCrash,
  Users,
} from "lucide-react";
import { Topbar } from "@/components/layout/topbar";
import { Badge } from "@/components/ui/badge";
import { Card } from "@/components/ui/card";
import { EmptyState } from "@/components/ui/empty-state";
import { Skeleton } from "@/components/ui/skeleton";
import { ApiError } from "@/lib/api/client";
import { useTeams, useTeamProjects, useMembers } from "@/lib/hooks/use-teams";
import { CreateProjectDialog } from "@/features/projects/create-project-dialog";
import type { Project, Team } from "@/types/project";

/** Team — the manager+ dashboard: pick a team, see its projects rolled up by
 *  status, create a project, and read the team's membership. Assigning a workflow
 *  to a member happens inside a project's workflow board (the existing
 *  workflow→tasks screens), reached by drilling into the project from Projects. */
export function TeamDashboard() {
  const { data: teams, isLoading, error } = useTeams();
  const [teamId, setTeamId] = useState<string | null>(null);

  // Default to the first team once loaded.
  useEffect(() => {
    if (!teamId && teams && teams.length > 0) setTeamId(teams[0].id);
  }, [teams, teamId]);

  const team = teams?.find((t) => t.id === teamId) ?? null;

  return (
    <div className="flex h-full flex-col">
      <Topbar
        title="Team"
        subtitle={team ? team.title : "Team dashboard"}
        actions={teamId ? <CreateProjectDialog team={teamId} /> : undefined}
      />
      <div className="flex-1 overflow-auto p-5">
        {error ? (
          <EmptyState
            icon={ServerCrash}
            title="Can't load teams"
            description={error instanceof ApiError ? error.message : "Unexpected error"}
          />
        ) : isLoading && !teams ? (
          <div className="space-y-4">
            <Skeleton className="h-9 w-64" />
            <Skeleton className="h-24 w-full" />
          </div>
        ) : !teams || teams.length === 0 ? (
          <EmptyState
            icon={Users}
            title="No teams yet"
            description="Teams own projects and carry membership. An admin creates teams in the Admin area."
          />
        ) : (
          <div className="space-y-6">
            <TeamPicker teams={teams} value={teamId} onChange={setTeamId} />
            {team && <TeamPanel team={team} />}
          </div>
        )}
      </div>
    </div>
  );
}

function TeamPicker({
  teams,
  value,
  onChange,
}: {
  teams: Team[];
  value: string | null;
  onChange: (id: string) => void;
}) {
  return (
    <div className="flex flex-wrap gap-2">
      {teams.map((t) => (
        <button
          key={t.id}
          onClick={() => onChange(t.id)}
          className={
            "rounded-md border px-3 py-1.5 text-xs font-medium transition-colors " +
            (value === t.id
              ? "border-accent/40 bg-accent-soft/50 text-accent"
              : "border-border bg-surface-2 text-muted-foreground hover:text-foreground")
          }
        >
          {t.title}
        </button>
      ))}
    </div>
  );
}

function TeamPanel({ team }: { team: Team }) {
  const { data: projects } = useTeamProjects(team.id);
  const { data: members } = useMembers(team.id);

  const active = projects?.filter((p) => p.status === "active") ?? [];
  const archived = projects?.filter((p) => p.status === "archived") ?? [];
  const repos = new Set((projects ?? []).flatMap((p) => p.repos)).size;
  const managers = members?.filter((m) => m.role === "manager").length ?? 0;

  return (
    <div className="space-y-6">
      {/* Status roll-up. */}
      <div className="grid grid-cols-2 gap-3 sm:grid-cols-4">
        <Stat label="Active projects" value={active.length} />
        <Stat label="Archived" value={archived.length} />
        <Stat label="Repos in play" value={repos} />
        <Stat label="Members" value={members?.length ?? 0} sub={`${managers} manager${managers === 1 ? "" : "s"}`} />
      </div>

      {/* The team's projects. */}
      <div>
        <p className="mb-2 text-xs font-medium text-muted-foreground">Projects</p>
        {!projects ? (
          <Skeleton className="h-24 w-full" />
        ) : projects.length === 0 ? (
          <EmptyState
            icon={FolderKanban}
            title="No projects in this team yet"
            description="Create the first project — it becomes the home for this team's workflows."
            action={<CreateProjectDialog team={team.id} />}
          />
        ) : (
          <div className="grid grid-cols-1 gap-3 sm:grid-cols-2 lg:grid-cols-3">
            {[...active, ...archived].map((p) => (
              <TeamProjectCard key={p.id} project={p} />
            ))}
          </div>
        )}
      </div>

      {/* Membership roll-up (read-only here; managed in Admin). */}
      <div>
        <p className="mb-2 text-xs font-medium text-muted-foreground">Members</p>
        {!members ? (
          <Skeleton className="h-16 w-full" />
        ) : members.length === 0 ? (
          <p className="text-xs text-muted-foreground">
            No members yet — an admin adds them in the Admin area.
          </p>
        ) : (
          <div className="flex flex-wrap gap-2">
            {members.map((m) => (
              <Badge key={m.user} variant={m.role === "manager" ? "accent" : "outline"}>
                {m.user} · {m.role}
              </Badge>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}

function TeamProjectCard({ project }: { project: Project }) {
  return (
    <Card className="p-4">
      <div className="flex items-start justify-between gap-2">
        <div className="min-w-0 flex-1">
          <p className="truncate text-sm font-medium">{project.title}</p>
          <p className="truncate font-mono text-[10px] text-muted-foreground">{project.id}</p>
        </div>
        <Badge variant={project.status === "active" ? "accent" : "outline"}>
          {project.status}
        </Badge>
      </div>
      {project.repos.length > 0 && (
        <div className="mt-3">
          <Badge>
            <FolderGit2 className="size-3" /> {project.repos.length} repo
            {project.repos.length === 1 ? "" : "s"}
          </Badge>
        </div>
      )}
    </Card>
  );
}

function Stat({ label, value, sub }: { label: string; value: number; sub?: string }) {
  return (
    <Card className="p-4">
      <p className="text-2xl font-semibold tabular-nums">{value}</p>
      <p className="text-xs text-muted-foreground">{label}</p>
      {sub && <p className="text-[10px] text-muted-foreground/70">{sub}</p>}
    </Card>
  );
}
