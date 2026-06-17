import type { Status } from "./task";
import {
  CircleDashed,
  CircleDot,
  Loader,
  ShieldCheck,
  CheckCircle2,
  AlertTriangle,
  type LucideIcon,
} from "lucide-react";

/** Display metadata for each lifecycle status: label, icon, and token color. */
export interface StatusMeta {
  label: string;
  icon: LucideIcon;
  /** CSS color variable (a `--color-status-*` token). */
  color: string;
  description: string;
}

export const STATUS_META: Record<Status, StatusMeta> = {
  pending: {
    label: "Pending",
    icon: CircleDashed,
    color: "var(--color-status-pending)",
    description: "Imported; waiting on dependencies",
  },
  ready: {
    label: "Ready",
    icon: CircleDot,
    color: "var(--color-status-ready)",
    description: "Deps met; eligible to be claimed",
  },
  running: {
    label: "Running",
    icon: Loader,
    color: "var(--color-status-running)",
    description: "Claimed by an agent; work in flight",
  },
  gating: {
    label: "Gating",
    icon: ShieldCheck,
    color: "var(--color-status-gating)",
    description: "Agent done; re-running the gate",
  },
  done: {
    label: "Done",
    icon: CheckCircle2,
    color: "var(--color-status-done)",
    description: "Committed, pushed, gate ran green",
  },
  blocked: {
    label: "Blocked",
    icon: AlertTriangle,
    color: "var(--color-status-blocked)",
    description: "Unrecoverable; reason recorded",
  },
};
