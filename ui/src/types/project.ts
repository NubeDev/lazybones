/** Mirrors of the cloud team graph (`lazybones_store`): the org hierarchy
 *  org → team → project → workflow, plus membership and the role model. See
 *  docs/lazybones-server/projects.md. */

/** A project's lifecycle (`lazybones_store::ProjectStatus`). */
export type ProjectStatus = "active" | "archived";

/** Mirror of `lazybones_store::Project` — the ownership/authz root that holds a
 *  team's workflows. */
export interface Project {
  id: string;
  title: string;
  status: ProjectStatus;
  /** Denormalized owning-team id (mirrors `project ->under-> team`); null until
   *  the project is placed under a team. */
  team: string | null;
  /** The repo target(s) this project's work spans. */
  repos: string[];
  created_at: string;
  updated_at: string;
}

/** Mirror of `lazybones_store::Team` — the mid container (owns projects). */
export interface Team {
  id: string;
  title: string;
  created_at: string;
  updated_at: string;
}

/** A member's authority within a team (`lazybones_store::MemberRole`). The global
 *  `admin` authority is a flag on the user, not a value here. */
export type MemberRole = "manager" | "member";

/** Mirror of `lazybones_store::Membership` — one `user ->member_of-> team` edge. */
export interface Membership {
  user: string;
  role: MemberRole;
}
