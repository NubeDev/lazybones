/** Mirror of `lazybones_store::SkillParam` — one structured-action parameter. */
export interface SkillParam {
  name: string;
  required: boolean;
  description: string;
}

/** Mirror of `lazybones_store::SkillAction` — an optional typed action a skill
 *  exposes for deterministic execution (scope §6.1). */
export interface SkillAction {
  method: "POST" | "PUT" | "DELETE";
  path_template: string;
  body_template?: unknown;
  params: SkillParam[];
}

/** Mirror of `lazybones_store::Skill` — a reusable block of agent instructions. */
export interface Skill {
  id: string;
  title: string;
  description: string;
  body: string;
  /** An optional typed action; absent for a plain markdown-runbook skill. */
  action?: SkillAction | null;
  created_at: string;
  updated_at: string;
}

/** Mirror of `lazybones_store::Attachment` — a generic owner→thing link.
 *  Both ends are `(kind, id)` strings, so any entity can own any thing-kind. */
export interface Attachment {
  id: string;
  owner_kind: string;
  owner_id: string;
  thing_kind: string;
  thing_id: string;
  created_at: string;
}
