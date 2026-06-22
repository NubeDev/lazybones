import {
  LayoutDashboard,
  ListTodo,
  History,
  Settings,
  FileText,
  Sparkles,
  Workflow,
  BookOpen,
  Palette,
  FolderKanban,
  Users,
  ShieldCheck,
} from "lucide-react";
import type { LucideIcon } from "lucide-react";

/** The top-level views. A tiny hash-free, in-memory router drives these. */
export type View =
  | "dashboard"
  | "projects"
  | "team"
  | "admin"
  | "templates"
  | "skills"
  | "workflows"
  | "tasks"
  | "runs"
  | "documents"
  | "branding"
  | "settings";

/** Which org-graph authority a nav entry demands. Absent ⇒ everyone (and, in
 *  operator mode with no roles, every gate opens — see `useRole`). */
export type NavGate = "manager" | "admin";

export interface NavItem {
  view: View;
  label: string;
  icon: LucideIcon;
  /** Role required to see this entry; omitted means everyone. */
  gate?: NavGate;
}

export const NAV_ITEMS: NavItem[] = [
  { view: "dashboard", label: "Dashboard", icon: LayoutDashboard },
  { view: "projects", label: "Projects", icon: FolderKanban },
  { view: "team", label: "Team", icon: Users, gate: "manager" },
  { view: "admin", label: "Admin", icon: ShieldCheck, gate: "admin" },
  { view: "templates", label: "Templates", icon: FileText },
  { view: "skills", label: "Skills", icon: Sparkles },
  { view: "workflows", label: "Workflows", icon: Workflow },
  { view: "tasks", label: "Tasks", icon: ListTodo },
  { view: "documents", label: "Documents", icon: BookOpen },
  { view: "branding", label: "Branding", icon: Palette },
  { view: "runs", label: "Run history", icon: History },
  { view: "settings", label: "Settings", icon: Settings },
];
