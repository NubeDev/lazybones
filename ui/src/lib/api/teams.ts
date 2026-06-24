import { request } from "./client";
import type { Membership, MemberRole, Project, Team } from "@/types/project";

/** `GET /teams` — every team (open read). */
export function listTeams(signal?: AbortSignal): Promise<Team[]> {
  return request<Team[]>("/teams", { signal });
}

/** `GET /teams/:id` — one team; `404` if absent. */
export function getTeam(id: string, signal?: AbortSignal): Promise<Team> {
  return request<Team>(`/teams/${encodeURIComponent(id)}`, { signal });
}

/** `POST /teams` — create (or re-affirm) a team. Requires `Author` + admin. */
export function createTeam(id: string, title: string): Promise<Team> {
  return request<Team>("/teams", { method: "POST", auth: true, body: { id, title } });
}

/** `GET /teams/:id/projects` — the projects placed `under` this team. */
export function listTeamProjects(id: string, signal?: AbortSignal): Promise<Project[]> {
  return request<Project[]>(`/teams/${encodeURIComponent(id)}/projects`, { signal });
}

/** `GET /teams/:id/members` — the team's members with their per-team roles. */
export function listMembers(team: string, signal?: AbortSignal): Promise<Membership[]> {
  return request<Membership[]>(`/teams/${encodeURIComponent(team)}/members`, { signal });
}

/** `POST /teams/:id/members` — add (or re-affirm) a member with a role. Requires
 *  `Author` + admin. */
export function addMember(team: string, user: string, role: MemberRole): Promise<Membership> {
  return request<Membership>(`/teams/${encodeURIComponent(team)}/members`, {
    method: "POST",
    auth: true,
    body: { user, role },
  });
}

/** `DELETE /teams/:id/members/:user` — remove a membership. Requires `Author` +
 *  admin. */
export function removeMember(team: string, user: string): Promise<{ removed: boolean }> {
  return request<{ removed: boolean }>(
    `/teams/${encodeURIComponent(team)}/members/${encodeURIComponent(user)}`,
    { method: "DELETE", auth: true },
  );
}
