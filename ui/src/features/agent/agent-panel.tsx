import { useEffect, useRef, useState } from "react";
import { Bot, History, Plus, Send, MessagesSquare, X } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Markdown } from "@/components/ui/markdown";
import { ScrollArea } from "@/components/ui/scroll-area";
import { relativeTime } from "@/lib/utils/platform";
import { cn } from "@/lib/utils/cn";
import { useAgentChat, useAgentConversations } from "@/lib/hooks/use-agent-chat";
import { useAgentContext } from "./agent-context";
import { AgentConfirmCard } from "./agent-confirm-card";
import type { AgentConversation } from "@/types/agent-chat";
import type { AgentMessage, AgentRole } from "@/types/agent-chat";

/** The global Lazybones-Agent chat panel: a docked right slide-over that overlays
 *  any page. Reuses the task-chat bubble/composer presentation, but talks to the
 *  global agent surface with per-conversation SSE streaming (scope §8).
 *
 *  GUARDRAIL (scope §9, §10.2): the agent authors and asks. It can *propose*
 *  lifecycle actions (on the "Author & manage" profile), but it never takes them:
 *  each proposal renders a Confirm/Cancel card, and the confirmed call is issued
 *  by the UI under the operator's token — never the agent's. */
export function AgentPanel({ onClose }: { onClose: () => void }) {
  const { context } = useAgentContext();
  const {
    conversation,
    messages,
    connected,
    sending,
    error,
    send,
    openConversation,
    newConversation,
  } = useAgentChat();
  const { data: conversations } = useAgentConversations();
  const [text, setText] = useState("");
  const [historyOpen, setHistoryOpen] = useState(false);
  const endRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    endRef.current?.scrollIntoView({ block: "end" });
  }, [messages.length]);

  const submit = () => {
    const t = text.trim();
    if (!t || sending) return;
    void send(t, context);
    setText("");
  };

  return (
    <aside className="flex h-full w-[22rem] shrink-0 flex-col border-l border-border bg-surface">
      <header className="flex h-14 shrink-0 items-center justify-between gap-1 border-b border-border px-4">
        <div className="flex min-w-0 items-center gap-2">
          <Bot className="size-4 text-accent" />
          <div className="min-w-0">
            <h2 className="truncate text-sm font-semibold">Lazybones Agent</h2>
            <p className="truncate text-[11px] text-muted-foreground">
              {connected ? "Connected" : conversation ? "Connecting…" : "New conversation"}
              {context.workflow_id ? ` · workflow ${context.workflow_id}` : ""}
              {context.task_id ? ` · task ${context.task_id}` : ""}
            </p>
          </div>
        </div>
        <div className="flex shrink-0 items-center gap-0.5">
          <Button
            size="icon"
            variant="ghost"
            onClick={() => setHistoryOpen((o) => !o)}
            title="Past conversations"
            className={cn("size-7", historyOpen && "text-accent")}
          >
            <History className="size-4" />
          </Button>
          <Button
            size="icon"
            variant="ghost"
            onClick={() => {
              newConversation();
              setHistoryOpen(false);
            }}
            title="New conversation"
            className="size-7"
          >
            <Plus className="size-4" />
          </Button>
          <Button
            size="icon"
            variant="ghost"
            onClick={onClose}
            title="Close"
            className="size-7"
          >
            <X className="size-4" />
          </Button>
        </div>
      </header>

      {historyOpen && (
        <ConversationList
          conversations={conversations ?? []}
          currentId={conversation}
          onPick={(id) => {
            openConversation(id);
            setHistoryOpen(false);
          }}
        />
      )}

      <ScrollArea className="flex-1">
        <div className="space-y-2 p-3">
          {messages.length === 0 && (
            <div className="flex flex-col items-center gap-1 py-10 text-center">
              <MessagesSquare className="size-5 text-muted-foreground/60" />
              <p className="text-[11px] text-muted-foreground">
                Ask me to author a workflow, task, template, or skill — or to
                explain the current state. I author; you press Start.
              </p>
            </div>
          )}
          {messages.map((m, i) => (
            <MessageItem key={`${m.at}-${i}`} message={m} />
          ))}
          <div ref={endRef} />
        </div>
      </ScrollArea>

      {error && <p className="px-3 pb-1 text-[11px] text-status-blocked">{error}</p>}

      <div className="flex items-end gap-2 border-t border-border p-3">
        <textarea
          value={text}
          disabled={sending}
          onChange={(e) => setText(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" && !e.shiftKey) {
              e.preventDefault();
              submit();
            }
          }}
          placeholder="Message the Lazybones Agent…"
          rows={2}
          className={cn(
            "flex-1 resize-none rounded-md border border-border bg-surface px-2.5 py-1.5 text-xs",
            "placeholder:text-muted-foreground/60 focus:border-border-strong focus:outline-none",
            "disabled:cursor-not-allowed disabled:opacity-60",
          )}
        />
        <Button size="sm" disabled={sending || !text.trim()} onClick={submit} title="Send (Enter)">
          <Send /> Send
        </Button>
      </div>
    </aside>
  );
}

/** The past-conversations switcher: newest first, click to reopen with history. */
function ConversationList({
  conversations,
  currentId,
  onPick,
}: {
  conversations: AgentConversation[];
  currentId: string | null;
  onPick: (id: string) => void;
}) {
  return (
    <div className="max-h-56 shrink-0 overflow-y-auto border-b border-border bg-surface-2/40">
      {conversations.length === 0 && (
        <p className="px-3 py-3 text-[11px] text-muted-foreground">
          No past conversations yet.
        </p>
      )}
      {conversations.map((c) => (
        <button
          key={c.id}
          onClick={() => onPick(c.id)}
          className={cn(
            "flex w-full flex-col items-start gap-0.5 border-b border-border/50 px-3 py-2 text-left transition-colors hover:bg-muted",
            c.id === currentId && "bg-accent-soft/40",
          )}
        >
          <span className="truncate text-xs font-medium">
            {conversationLabel(c)}
          </span>
          <span className="text-[10px] text-muted-foreground">
            {relativeTime(c.created_at)}
          </span>
        </button>
      ))}
    </div>
  );
}

/** A short label for a conversation, derived from the page it opened on. */
function conversationLabel(c: AgentConversation): string {
  const ctx = c.page_context;
  if (ctx?.workflow_id) return `Workflow ${ctx.workflow_id}`;
  if (ctx?.task_id) return `Task ${ctx.task_id}`;
  if (ctx?.selected_template_id) return `Template ${ctx.selected_template_id}`;
  if (ctx?.selected_skill_id) return `Skill ${ctx.selected_skill_id}`;
  if (ctx?.view) return `On ${ctx.view}`;
  return "Conversation";
}

/** Dispatch a message to the right renderer: a gated `confirm` proposal becomes
 *  a Confirm/Cancel card; everything else is a bubble. */
function MessageItem({ message }: { message: AgentMessage }) {
  if (message.role === "confirm" && message.action) {
    return <AgentConfirmCard summary={message.text} action={message.action} />;
  }
  return <Bubble role={message.role} text={message.text} at={message.at} />;
}

/** One message bubble. Operator turns right/accented, agent replies left/muted,
 *  and `tool` transparency notes centered/subtle. */
function Bubble({ role, text, at }: { role: AgentRole; text: string; at: string }) {
  if (role === "tool") {
    return (
      <div className="flex justify-center">
        <p className="max-w-[90%] whitespace-pre-wrap text-center text-[10px] italic text-muted-foreground/80">
          {text}
        </p>
      </div>
    );
  }
  const isUser = role === "user";
  return (
    <div className={cn("flex flex-col gap-0.5", isUser ? "items-end" : "items-start")}>
      <div
        className={cn(
          "max-w-[85%] min-w-0 rounded-lg px-2.5 py-1.5 text-xs leading-snug",
          isUser
            ? "whitespace-pre-wrap bg-primary text-primary-foreground"
            : "bg-muted text-foreground",
        )}
      >
        {/* Operator turns are plain text; the agent's replies are markdown
            (lists, tables, fenced code for JSON/text/etc.) via the shared
            Markdown renderer, which escapes raw HTML so this stays XSS-safe. */}
        {isUser ? text : <Markdown className="text-xs leading-snug">{text}</Markdown>}
      </div>
      <span className="px-1 text-[10px] text-muted-foreground/70">
        {isUser ? "you" : "agent"} · {relativeTime(at)}
      </span>
    </div>
  );
}
