import type { Status, Task } from "@/types/task";

/** The manual board moves a human (loop token) may actually drive over REST.
 *
 *  The backend only exposes two operator transitions — promote (`pending →
 *  ready`) and block (`* → blocked`). Everything else (claim, gate, done) is the
 *  hcom loop's job and is earned via a real green gate (SCOPE.md principle 4), so
 *  the board refuses to drop onto those columns rather than fire a doomed `409`. */
export type DragAction = "ready" | "block";

/** The action a drop implies, or `null` if `from → to` is not an operator move.
 *
 *  `pending → ready` requires every dependency to be `done`; an unready task
 *  can't be forced ready from the board (the loop promotes it once deps land). */
export function dropAction(task: Task, to: Status, byId: Map<string, Task>): DragAction | null {
  if (to === task.status) return null;
  if (to === "blocked") {
    // Terminal tasks have nothing to block.
    return task.status === "done" || task.status === "blocked" ? null : "block";
  }
  if (task.status === "pending" && to === "ready" && depsSatisfied(task, byId)) {
    return "ready";
  }
  return null;
}

/** Whether every dependency of `task` is `done` (so it may become `ready`). */
export function depsSatisfied(task: Task, byId: Map<string, Task>): boolean {
  return task.deps.every((d) => byId.get(d)?.status === "done");
}

/** Why a pending card can't yet be promoted to `ready`, or `null` if it can be
 *  (or isn't pending). Used to dim the card and explain the block in a tooltip,
 *  so a refused drop reads as a dependency rule rather than a broken board. */
export function promoteBlockedReason(task: Task, byId: Map<string, Task>): string | null {
  if (task.status !== "pending") return null;
  const pending = task.deps.filter((d) => byId.get(d)?.status !== "done");
  if (pending.length === 0) return null;
  return `Blocked: waiting on ${pending.join(", ")}`;
}

/** Whether a card in `from` can be dragged to `to` at all — used to highlight
 *  valid drop targets while a drag is in flight (deps are checked on drop). */
export function isPlausibleTarget(from: Status, to: Status): boolean {
  if (from === to) return false;
  const terminal = from === "done" || from === "blocked";
  if (terminal) return false;
  return to === "blocked" || (from === "pending" && to === "ready");
}
