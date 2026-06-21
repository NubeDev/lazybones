import type { View } from "@/app/navigation";

/** The page the operator is viewing, sent with every Lazybones-Agent turn so the
 *  agent is grounded in the workflow/task/page in scope (scope §7). Every field
 *  is optional — the server renders only what's present and still re-reads
 *  authoritative state via GET before acting on any id. */
export interface PageContext {
  /** Which page the operator is on. */
  view?: View;
  /** The workflow in scope (a workflow detail panel). */
  workflow_id?: string;
  /** The task in scope (a task detail panel). */
  task_id?: string;
  /** The run in scope. */
  run_id?: string;
  /** The repo of the workflow in scope (a default for authoring). */
  repo?: string;
  /** The base branch of the workflow in scope. */
  base_branch?: string;
  /** A selected template id, if any. */
  selected_template_id?: string;
  /** A selected skill id, if any. */
  selected_skill_id?: string;
}
