import { request } from "./client";
import type {
  AgentChatPosted,
  AgentConversation,
  AgentMessage,
} from "@/types/agent-chat";
import type { PageContext } from "@/types/page-context";

/** `POST /agent/chat` — submit one operator turn. Opens a conversation when
 *  `conversation` is absent. The agent's reply arrives over the per-conversation
 *  SSE stream, not in this response. */
export function postAgentChat(args: {
  conversation?: string;
  text: string;
  pageContext?: PageContext;
}): Promise<AgentChatPosted> {
  return request<AgentChatPosted>("/agent/chat", {
    method: "POST",
    body: {
      conversation: args.conversation,
      text: args.text,
      page_context: args.pageContext,
    },
  });
}

/** `GET /agent/chat/:conversation` — the conversation's messages, oldest first.
 *  `404` if the conversation is unknown. */
export function getAgentChat(
  conversation: string,
  signal?: AbortSignal,
): Promise<AgentMessage[]> {
  return request<AgentMessage[]>(
    `/agent/chat/${encodeURIComponent(conversation)}`,
    { signal },
  );
}

/** `GET /agent/conversations` — list conversations, newest first. */
export function listAgentConversations(
  signal?: AbortSignal,
): Promise<AgentConversation[]> {
  return request<AgentConversation[]>("/agent/conversations", { signal });
}

/** Issue a confirmed gated lifecycle action — the exact REST call the agent
 *  proposed — under the operator's loop token (scope §10.2). The agent never
 *  makes this call; the UI does, only after the human clicks Confirm. */
export function confirmAgentAction(action: {
  method: "POST" | "PUT" | "DELETE";
  path: string;
  body?: unknown;
}): Promise<unknown> {
  return request<unknown>(action.path, {
    method: action.method,
    auth: true,
    body: action.body,
  });
}
