import { request } from "./client";
import type { ChatMessage, ChatPosted } from "@/types/chat";

/** `GET /tasks/:id/chat` — the task's conversation, oldest first. Open (no auth),
 *  like the hcom log / transcript reads. `404` if the task is unknown. */
export function listChat(id: string, signal?: AbortSignal): Promise<ChatMessage[]> {
  return request<ChatMessage[]>(`/tasks/${encodeURIComponent(id)}/chat`, { signal });
}

/** `POST /tasks/:id/chat` — post a message to the task's agent. Stored durably
 *  first, then acted on by task state: delivered live (running), revived
 *  (blocked → re-spawned with the conversation in its prompt), or stored as
 *  guidance (pending/ready). `409` if the task is done. Requires `Block`. */
export function postChat(id: string, text: string): Promise<ChatPosted> {
  return request<ChatPosted>(`/tasks/${encodeURIComponent(id)}/chat`, {
    method: "POST",
    auth: true,
    body: { text },
  });
}
