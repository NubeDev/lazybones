/** Mirror of `lazybones_store::ChatRole`. */
export type ChatRole = "user" | "agent";

/** Mirror of `lazybones_store::ChatMessage` — one message in a task's
 *  conversation (oldest first when listed). */
export interface ChatMessage {
  /** The run (workflow id) this conversation belongs to — grouping only. */
  run: string;
  /** The task whose thread this message is on. */
  task: string;
  /** Who wrote it. */
  role: ChatRole;
  /** The message text. */
  text: string;
  /** RFC3339 timestamp. */
  at: string;
}

/** What `POST /tasks/:id/chat` did with the message, given the task's state:
 *  - `delivered` — sent live to the running agent's hcom thread;
 *  - `revived` — a blocked task was revived; the next tick re-spawns it;
 *  - `stored` — recorded as guidance, folded into the prompt at the next claim. */
export type ChatDelivery = "delivered" | "revived" | "stored";

/** Mirror of the `POST /tasks/:id/chat` response. */
export interface ChatPosted {
  message: ChatMessage;
  delivery: ChatDelivery;
}
