import {
  FileEdit,
  CircleDot,
  Loader,
  AlertTriangle,
  CheckCircle2,
  PauseCircle,
  type LucideIcon,
} from "lucide-react";
import type { WorkflowState } from "./workflow";

/** Display metadata for each derived workflow state. The state itself is computed
 *  server-side and never recomputed here — this only maps it to a label + color. */
export interface WorkflowStateMeta {
  label: string;
  icon: LucideIcon;
  /** A `--color-status-*` token reused from the task palette. */
  color: string;
  description: string;
}

export const WORKFLOW_STATE_META: Record<WorkflowState, WorkflowStateMeta> = {
  draft: {
    label: "Draft",
    icon: FileEdit,
    color: "var(--color-status-pending)",
    description: "No task promoted yet",
  },
  ready: {
    label: "Ready",
    icon: CircleDot,
    color: "var(--color-status-ready)",
    description: "Tasks promoted; waiting to be claimed",
  },
  running: {
    label: "Running",
    icon: Loader,
    color: "var(--color-status-running)",
    description: "A task is running or gating",
  },
  "needs-attention": {
    label: "Needs attention",
    icon: AlertTriangle,
    color: "var(--color-status-blocked)",
    description: "A task is blocked",
  },
  done: {
    label: "Done",
    icon: CheckCircle2,
    color: "var(--color-status-done)",
    description: "Every task is done",
  },
  stopped: {
    label: "Stopped",
    icon: PauseCircle,
    color: "var(--color-status-pending)",
    description: "Paused by a human — resume to continue",
  },
};
