import { request } from "./client";
import type { Project } from "@/types/project";

/** The authored fields of a project (everything but status + timestamps). */
export interface ProjectDraft {
  id: string;
  title: string;
  /** The owning team; omit/null for a teamless (admin-only) project. */
  team?: string | null;
  /** The repo target(s) this project's work spans. */
  repos: string[];
}

/** `GET /projects` — all projects (open read), optionally scoped to `?team=`. */
export function listProjects(team?: string, signal?: AbortSignal): Promise<Project[]> {
  const q = team ? `?team=${encodeURIComponent(team)}` : "";
  return request<Project[]>(`/projects${q}`, { signal });
}

/** `GET /projects/:id` — one project; `404` if absent. */
export function getProject(id: string, signal?: AbortSignal): Promise<Project> {
  return request<Project>(`/projects/${encodeURIComponent(id)}`, { signal });
}

/** `POST /projects` — author a project. Requires `Author` + manager of the owning
 *  team (or admin). `409` if the id is taken. */
export function createProject(draft: ProjectDraft): Promise<Project> {
  return request<Project>("/projects", {
    method: "POST",
    auth: true,
    body: { id: draft.id, title: draft.title, team: draft.team ?? null, repos: draft.repos },
  });
}

/** `PUT /projects/:id` — overwrite a project's authored fields (title, repos).
 *  `status` and the owning team are preserved. */
export function updateProject(
  id: string,
  body: { title: string; repos: string[] },
): Promise<Project> {
  return request<Project>(`/projects/${encodeURIComponent(id)}`, {
    method: "PUT",
    auth: true,
    body,
  });
}

/** `POST /projects/:id/archive` — shelve a project (`status → archived`). */
export function archiveProject(id: string): Promise<Project> {
  return request<Project>(`/projects/${encodeURIComponent(id)}/archive`, {
    method: "POST",
    auth: true,
  });
}
