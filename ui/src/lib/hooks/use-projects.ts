import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  archiveProject,
  createProject,
  getProject,
  listProjects,
  updateProject,
  type ProjectDraft,
} from "@/lib/api/projects";

/** Poll the project list (open read), optionally scoped to one team. */
export function useProjects(team?: string) {
  return useQuery({
    queryKey: ["projects", team ?? null],
    queryFn: ({ signal }) => listProjects(team, signal),
    refetchInterval: 10000,
  });
}

/** Fetch one project. Disabled when `id` is absent. */
export function useProject(id?: string) {
  return useQuery({
    queryKey: ["project", id],
    queryFn: ({ signal }) => getProject(id as string, signal),
    enabled: id != null,
  });
}

/** Author a new project (`POST /projects`). */
export function useCreateProject() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (draft: ProjectDraft) => createProject(draft),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["projects"] }),
  });
}

/** Edit a project's authored fields (`PUT /projects/:id`). */
export function useUpdateProject() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ id, title, repos }: { id: string; title: string; repos: string[] }) =>
      updateProject(id, { title, repos }),
    onSuccess: (_data, { id }) => {
      qc.invalidateQueries({ queryKey: ["projects"] });
      qc.invalidateQueries({ queryKey: ["project", id] });
    },
  });
}

/** Archive a project (`POST /projects/:id/archive`). */
export function useArchiveProject() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => archiveProject(id),
    onSuccess: (_data, id) => {
      qc.invalidateQueries({ queryKey: ["projects"] });
      qc.invalidateQueries({ queryKey: ["project", id] });
    },
  });
}
