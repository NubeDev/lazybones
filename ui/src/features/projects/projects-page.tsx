import { useState } from "react";
import {
  Archive,
  ArrowLeft,
  FolderGit2,
  FolderKanban,
  ServerCrash,
} from "lucide-react";
import { Topbar } from "@/components/layout/topbar";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Card } from "@/components/ui/card";
import { EmptyState } from "@/components/ui/empty-state";
import { Skeleton } from "@/components/ui/skeleton";
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogTrigger,
} from "@/components/ui/dialog";
import { ApiError } from "@/lib/api/client";
import { useProjects, useArchiveProject } from "@/lib/hooks/use-projects";
import { useRole } from "@/lib/hooks/use-role";
import { WorkflowsPage } from "@/features/workflows/workflows-page";
import { CreateProjectDialog } from "@/features/projects/create-project-dialog";
import type { Project } from "@/types/project";

/** Projects — everyone's home: *my projects → workflows → tasks*. The list
 *  (with create, manager-gated) drills into a single project, where the existing
 *  workflow→tasks board, task detail, chat and hcom-log screens nest unchanged.
 *
 *  Selection lives here (no URL router) so list ↔ drill-down stays in-memory,
 *  matching the Workflows/Branding views. */
export function ProjectsPage() {
  const [selected, setSelected] = useState<string | null>(null);

  if (selected) {
    const project = selected;
    return <ProjectDetail id={project} onBack={() => setSelected(null)} />;
  }
  return <ProjectsList onOpen={setSelected} />;
}

function ProjectsList({ onOpen }: { onOpen: (id: string) => void }) {
  const { data: projects, isLoading, error } = useProjects();
  const { isManager } = useRole();

  const active = projects?.filter((p) => p.status === "active") ?? [];
  const archived = projects?.filter((p) => p.status === "archived") ?? [];

  const subtitle = projects
    ? `${active.length} active project${active.length === 1 ? "" : "s"}`
    : "Loading…";

  // A manager (or operator) may author projects; a plain member only sees theirs.
  const newButton = isManager ? <CreateProjectDialog onCreated={onOpen} /> : null;

  return (
    <div className="flex h-full flex-col">
      <Topbar title="Projects" subtitle={subtitle} actions={newButton} />
      <div className="flex-1 overflow-auto p-5">
        {error ? (
          <EmptyState
            icon={ServerCrash}
            title="Can't load projects"
            description={error instanceof ApiError ? error.message : "Unexpected error"}
          />
        ) : isLoading && !projects ? (
          <div className="grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-3">
            {Array.from({ length: 3 }).map((_, i) => (
              <Skeleton key={i} className="h-28 w-full" />
            ))}
          </div>
        ) : !projects || projects.length === 0 ? (
          <EmptyState
            icon={FolderKanban}
            title="No projects yet"
            description="A project is the long-running home for a team's work — it holds a stream of workflows over months. Create one, then add workflows inside it."
            action={newButton ?? undefined}
          />
        ) : (
          <div className="space-y-6">
            <div className="grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-3">
              {active.map((p) => (
                <ProjectCard key={p.id} project={p} onOpen={() => onOpen(p.id)} />
              ))}
            </div>
            {archived.length > 0 && (
              <div>
                <p className="mb-2 text-xs font-medium text-muted-foreground">Archived</p>
                <div className="grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-3">
                  {archived.map((p) => (
                    <ProjectCard key={p.id} project={p} onOpen={() => onOpen(p.id)} />
                  ))}
                </div>
              </div>
            )}
          </div>
        )}
      </div>
    </div>
  );
}

function ProjectCard({ project, onOpen }: { project: Project; onOpen: () => void }) {
  return (
    <Card
      className="cursor-pointer p-4 transition-colors hover:border-border-strong"
      onClick={onOpen}
    >
      <div className="flex items-start justify-between gap-2">
        <div className="min-w-0 flex-1">
          <p className="truncate text-sm font-medium">{project.title}</p>
          <p className="truncate font-mono text-[10px] text-muted-foreground">{project.id}</p>
        </div>
        <Badge variant={project.status === "active" ? "accent" : "outline"}>
          {project.status}
        </Badge>
      </div>
      <div className="mt-3 flex flex-wrap items-center gap-1.5">
        {project.team && <Badge variant="outline">team · {project.team}</Badge>}
        {project.repos.length > 0 && (
          <Badge>
            <FolderGit2 className="size-3" /> {project.repos.length} repo
            {project.repos.length === 1 ? "" : "s"}
          </Badge>
        )}
      </div>
    </Card>
  );
}

/** A single project's drill-down: the project header + archive, then the existing
 *  workflows board nested unchanged (workflow → tasks → detail/chat/hcom log). */
function ProjectDetail({ id, onBack }: { id: string; onBack: () => void }) {
  const { data: projects } = useProjects();
  const project = projects?.find((p) => p.id === id);
  const { isManager } = useRole();
  const archive = useArchiveProject();

  return (
    <div className="flex h-full min-w-0 flex-col">
      <Topbar
        title={project?.title ?? id}
        subtitle={
          project
            ? [project.id, project.team ? `team · ${project.team}` : null]
                .filter(Boolean)
                .join(" · ")
            : "Project"
        }
        actions={
          <div className="flex items-center gap-2">
            <Button variant="ghost" size="sm" onClick={onBack}>
              <ArrowLeft /> Projects
            </Button>
            {isManager && project && project.status === "active" && (
              <ArchiveButton
                project={project}
                pending={archive.isPending}
                onConfirm={() => archive.mutate(project.id)}
              />
            )}
          </div>
        }
      />
      <div className="min-h-0 flex-1 overflow-hidden">
        {/* The workflow→tasks board, task detail, chat and hcom log are unchanged;
            they simply nest inside the project drill-down (projects.md One-UI). */}
        <WorkflowsPage />
      </div>
    </div>
  );
}

function ArchiveButton({
  project,
  pending,
  onConfirm,
}: {
  project: Project;
  pending: boolean;
  onConfirm: () => void;
}) {
  const [open, setOpen] = useState(false);
  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogTrigger asChild>
        <Button variant="ghost" size="sm" title="Archive project">
          <Archive /> Archive
        </Button>
      </DialogTrigger>
      <DialogContent
        title={`Archive ${project.id}?`}
        description="Shelves the project (status → archived), keeping it for history. Its workflows are untouched."
      >
        <div className="mt-2 flex justify-end gap-2">
          <DialogClose asChild>
            <Button variant="ghost" size="sm">
              Cancel
            </Button>
          </DialogClose>
          <Button
            variant="destructive"
            size="sm"
            disabled={pending}
            onClick={() => {
              setOpen(false);
              onConfirm();
            }}
          >
            <Archive /> Archive
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  );
}
