import { request } from "./client";
import type { Skill } from "@/types/skill";

/** `GET /skills` — all reusable skills (open read). */
export function listSkills(signal?: AbortSignal): Promise<Skill[]> {
  return request<Skill[]>("/skills", { signal });
}

/** `GET /skills/:id` — one skill; `404` if absent. */
export function getSkill(id: string, signal?: AbortSignal): Promise<Skill> {
  return request<Skill>(`/skills/${encodeURIComponent(id)}`, { signal });
}

/** The authored fields of a skill. */
export interface SkillDraft {
  title: string;
  description: string;
  body: string;
}

/** `POST /skills` — author a reusable skill. `409` if id taken. */
export function createSkill(id: string, draft: SkillDraft): Promise<Skill> {
  return request<Skill>("/skills", {
    method: "POST",
    auth: true,
    body: { id, ...draft },
  });
}

/** `PUT /skills/:id` — edit an existing skill. `404` if id is unknown.
 *  The id is fixed by the path; every other field is overwritten wholesale. */
export function updateSkill(id: string, draft: SkillDraft): Promise<Skill> {
  return request<Skill>(`/skills/${encodeURIComponent(id)}`, {
    method: "PUT",
    auth: true,
    body: draft,
  });
}

/** `DELETE /skills/:id` — remove a skill; returns whether it existed. */
export function deleteSkill(id: string): Promise<{ deleted: boolean }> {
  return request<{ deleted: boolean }>(`/skills/${encodeURIComponent(id)}`, {
    method: "DELETE",
    auth: true,
  });
}
