import { useSyncExternalStore } from "react";

/** The principal's effective role, mirroring the backend's role model
 *  (projects.md "Roles"): a global `admin`, a per-team `manager`, or a plain
 *  `member`. */
export type Role = "admin" | "manager" | "member";

/** Where the simulated/claimed role is parked. In a roles-enabled deployment the
 *  role rides in on the JWT; this UI has no `/me` endpoint yet, so the claim is
 *  read from here (a settings control or auth bootstrap writes it). Absent ⇒
 *  **operator mode**: no `[server]` config means no roles, so the UI shows
 *  everything exactly as the daemon trusts its single operator (guard.rs
 *  `ensure_role` no-ops locally). */
const ROLE_KEY = "lazybones-role";

/** The resolved authority of the current principal. */
export interface RoleInfo {
  /** The claimed role, or `null` in operator mode (no roles configured). */
  role: Role | null;
  /** True when no role claim is present — the local single-operator passthrough,
   *  where every section is visible. */
  operatorMode: boolean;
  /** Holds the global admin authority (admin, or operator mode). */
  isAdmin: boolean;
  /** Holds team-manager authority or above (manager/admin, or operator mode). */
  isManager: boolean;
  /** May see the Team section (manager+). */
  canSeeTeam: boolean;
  /** May see the Admin section (admin only). */
  canSeeAdmin: boolean;
}

function readRole(): Role | null {
  if (typeof localStorage === "undefined") return null;
  const raw = localStorage.getItem(ROLE_KEY);
  return raw === "admin" || raw === "manager" || raw === "member" ? raw : null;
}

/** Persist (or clear) the simulated role claim and notify subscribers. */
export function setRole(role: Role | null): void {
  if (typeof localStorage === "undefined") return;
  if (role) localStorage.setItem(ROLE_KEY, role);
  else localStorage.removeItem(ROLE_KEY);
  window.dispatchEvent(new Event(ROLE_EVENT));
}

const ROLE_EVENT = "lazybones-role-change";

function subscribe(cb: () => void): () => void {
  window.addEventListener("storage", cb);
  window.addEventListener(ROLE_EVENT, cb);
  return () => {
    window.removeEventListener("storage", cb);
    window.removeEventListener(ROLE_EVENT, cb);
  };
}

/** Read the principal's role and derive section visibility. Operator mode (no
 *  role configured) unlocks everything, matching the backend's local-no-roles
 *  passthrough. */
export function useRole(): RoleInfo {
  const role = useSyncExternalStore(subscribe, readRole, () => null);
  const operatorMode = role === null;
  const isAdmin = operatorMode || role === "admin";
  const isManager = operatorMode || role === "admin" || role === "manager";
  return {
    role,
    operatorMode,
    isAdmin,
    isManager,
    canSeeTeam: isManager,
    canSeeAdmin: isAdmin,
  };
}
