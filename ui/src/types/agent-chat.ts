import type { PageContext } from "./page-context";

/** Who authored a Lazybones-Agent message (mirror of
 *  `lazybones_store::AgentRole`). `tool` is a transparency note about a REST
 *  action the agent took; `confirm` is a gated lifecycle action the agent is
 *  *proposing* — the human confirms it in the UI (scope §10.2). */
export type AgentRole = "user" | "agent" | "tool" | "confirm";

/** A gated lifecycle action the agent proposes — the exact REST call the UI
 *  issues (under the operator's token) if the human confirms (mirror of
 *  `lazybones_store::ConfirmAction`). */
export interface ConfirmAction {
  /** A short verb for the label, e.g. `"start"`, `"retry"`, `"delete"`. */
  action: string;
  /** The HTTP method. */
  method: "POST" | "PUT" | "DELETE";
  /** The REST path, e.g. `"/workflows/x/start"`. */
  path: string;
  /** An optional JSON request body. */
  body?: unknown;
}

/** Mirror of `lazybones_store::AgentMessage` — one message in a conversation. */
export interface AgentMessage {
  conversation_id: string;
  role: AgentRole;
  /** The message text (the summary, for a `confirm` message). */
  text: string;
  /** The proposed gated action, present only for the `confirm` role. */
  action?: ConfirmAction | null;
  /** RFC3339 timestamp. */
  at: string;
}

/** Mirror of `lazybones_store::AgentConversation`. */
export interface AgentConversation {
  id: string;
  page_context: PageContext | null;
  created_at: string;
}

/** Mirror of the `POST /agent/chat` response. */
export interface AgentChatPosted {
  /** The conversation this turn belongs to (newly minted if none was sent). */
  conversation: string;
  /** The persisted operator message. */
  message: AgentMessage;
}
