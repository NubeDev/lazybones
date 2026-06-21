import { useCallback, useEffect, useRef, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { apiBase } from "@/lib/api/config";
import {
  getAgentChat,
  listAgentConversations,
  postAgentChat,
} from "@/lib/api/agent-chat";
import type { AgentMessage } from "@/types/agent-chat";
import type { PageContext } from "@/types/page-context";

/** The live state of one Lazybones-Agent conversation: the message list plus the
 *  send/connection status and conversation controls the panel renders. */
export interface AgentChatState {
  conversation: string | null;
  messages: AgentMessage[];
  connected: boolean;
  sending: boolean;
  /** True from the moment a turn is sent until the agent's reply (or a timeout
   *  note) arrives over SSE. Drives the panel's live "working…" indicator — the
   *  turn is fire-and-forget, so `sending` flips false almost immediately while
   *  the agent is still doing REST work for 5–180s. */
  working: boolean;
  /** The agent's latest live activity note while working ("Running a command…"),
   *  streamed from its hcom tool-status events; null when idle/unknown. */
  activity: string | null;
  error: string | null;
  /** Post a turn; opens a conversation on first send. */
  send: (text: string, pageContext?: PageContext) => Promise<void>;
  /** Switch to an existing conversation, replaying its history. */
  openConversation: (id: string) => void;
  /** Start a fresh conversation (clears the thread; the next send opens one). */
  newConversation: () => void;
}

/**
 * Drive one conversation: seed history from `GET /agent/chat/:id`, stream new
 * messages from the per-conversation SSE endpoint, and post turns. Pass an
 * `initialConversation` to reopen a past thread; otherwise the first `send`
 * opens one. `openConversation`/`newConversation` switch threads in place.
 */
export function useAgentChat(initialConversation?: string | null): AgentChatState {
  const [conversation, setConversation] = useState<string | null>(
    initialConversation ?? null,
  );
  const [messages, setMessages] = useState<AgentMessage[]>([]);
  const [connected, setConnected] = useState(false);
  const [sending, setSending] = useState(false);
  const [working, setWorking] = useState(false);
  const [activity, setActivity] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  // De-dupe by (role, at, text) so a seed + stream overlap doesn't double-render.
  const seen = useRef<Set<string>>(new Set());

  const append = useCallback((m: AgentMessage) => {
    const key = `${m.role}|${m.at}|${m.text}`;
    if (seen.current.has(key)) return;
    seen.current.add(key);
    setMessages((prev) => [...prev, m]);
  }, []);

  const openConversation = useCallback((id: string) => {
    seen.current = new Set();
    setMessages([]);
    setError(null);
    setConnected(false);
    setWorking(false);
    setActivity(null);
    setConversation(id);
  }, []);

  const newConversation = useCallback(() => {
    seen.current = new Set();
    setMessages([]);
    setError(null);
    setConnected(false);
    setWorking(false);
    setActivity(null);
    setConversation(null);
  }, []);

  // Safety net for the working indicator: a turn replies (or times out with a
  // note) within the runner's 180s budget, normally clearing `working` over SSE.
  // If the stream is down so no message arrives, auto-clear a bit past that
  // budget so the composer never stays locked forever.
  useEffect(() => {
    if (!working) return;
    const t = setTimeout(() => setWorking(false), 195_000);
    return () => clearTimeout(t);
  }, [working]);

  // Subscribe to the per-conversation SSE stream once a conversation exists.
  useEffect(() => {
    if (!conversation) return;
    let cancelled = false;
    let es: EventSource | null = null;

    async function seed() {
      try {
        const history = await getAgentChat(conversation as string);
        if (cancelled) return;
        seen.current = new Set(history.map((m) => `${m.role}|${m.at}|${m.text}`));
        setMessages(history);
        setConnected(true);
      } catch {
        // A fresh conversation may not have history yet; ignore.
      }
    }
    void seed();

    try {
      es = new EventSource(
        `${apiBase()}/agent/chat/${encodeURIComponent(conversation)}/stream`,
      );
    } catch {
      return () => {
        cancelled = true;
      };
    }

    es.addEventListener("open", () => setConnected(true));
    es.addEventListener("message", (ev) => {
      let msg: AgentMessage;
      try {
        msg = JSON.parse((ev as MessageEvent).data) as AgentMessage;
      } catch {
        return;
      }
      if (msg.conversation_id !== conversation) return;
      append(msg);
      // The agent has produced something for this turn — a reply, a confirm
      // proposal, or a runner note (e.g. the timeout note). Stop the working
      // indicator; the operator's own echoed turn (`user`) doesn't count.
      if (msg.role !== "user") {
        setWorking(false);
        setActivity(null);
      }
    });
    // Ephemeral "what the agent is doing now" ticks (not persisted).
    es.addEventListener("activity", (ev) => {
      try {
        const { text } = JSON.parse((ev as MessageEvent).data) as { text: string };
        if (text) setActivity(text);
      } catch {
        /* ignore malformed activity */
      }
    });
    es.addEventListener("error", () => setConnected(false));

    return () => {
      cancelled = true;
      es?.close();
    };
  }, [conversation, append]);

  const send = useCallback(
    async (text: string, pageContext?: PageContext) => {
      const trimmed = text.trim();
      if (!trimmed || sending) return;
      setSending(true);
      setError(null);
      try {
        const res = await postAgentChat({
          conversation: conversation ?? undefined,
          text: trimmed,
          pageContext,
        });
        if (!conversation) {
          // First turn: adopt the conversation and show the operator message
          // immediately (the stream will also carry it; the dedupe absorbs it).
          setConversation(res.conversation);
        }
        append(res.message);
        // The turn is now running off-request on the daemon; show "working…"
        // until its reply (or a timeout note) streams back over SSE.
        setWorking(true);
        setActivity(null);
      } catch (e) {
        setError(e instanceof Error ? e.message : "send failed");
        setWorking(false);
        setActivity(null);
      } finally {
        setSending(false);
      }
    },
    [conversation, sending, append],
  );

  return {
    conversation,
    messages,
    connected,
    sending,
    working,
    activity,
    error,
    send,
    openConversation,
    newConversation,
  };
}

/** The list of past conversations (newest first) for the history switcher. */
export function useAgentConversations() {
  return useQuery({
    queryKey: ["agent-conversations"],
    queryFn: ({ signal }) => listAgentConversations(signal),
    refetchInterval: 8000,
  });
}
