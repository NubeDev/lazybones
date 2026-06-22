import {
  LayoutDashboard,
  ListTodo,
  History,
  Settings,
  FileText,
  Sparkles,
  Workflow,
} from "lucide-react";
import type { LucideIcon } from "lucide-react";

/** The top-level views. A tiny hash-free, in-memory router drives these. */
export type View =
  | "dashboard"
  | "templates"
  | "skills"
  | "workflows"
  | "tasks"
  | "runs"
  | "settings";

export interface NavItem {
  view: View;
  label: string;
  icon: LucideIcon;
}

export const NAV_ITEMS: NavItem[] = [
  { view: "dashboard", label: "Dashboard", icon: LayoutDashboard },
  { view: "templates", label: "Templates", icon: FileText },
  { view: "skills", label: "Skills", icon: Sparkles },
  { view: "workflows", label: "Workflows", icon: Workflow },
  { view: "tasks", label: "Tasks", icon: ListTodo },
  { view: "runs", label: "Run history", icon: History },
  { view: "settings", label: "Settings", icon: Settings },
];
