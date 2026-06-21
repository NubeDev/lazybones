import { useEffect, useRef, useState } from "react";
import { Send, MessagesSquare } from "lucide-react";
import { Button } from "@/components/ui/button";
import { ScrollArea } from "@/components/ui/scroll-area";
import { ApiError } from "@/lib/api/client";
import { useChat, usePostChat } from "@/lib/hooks/use-chat";
import { relativeTime } from "@/lib/utils/platform";
import { cn } from "@/lib/utils/cn";
import type { ChatDelivery } from "@/types/chat";
import type { Task } from "@/types/task";

/** What a posted message did, phrased for the operator. Mirrors the backend's
 *  `delivery` values (chat.rs). */
const DELIVERY_NOTE: Record<ChatDelivery, string> = {
  delivered: "Sent to the running agent.",
  revived: "Revived — re-running the task with your guidance.",
  stored: "Saved — the agent will see it when it next picks up the task.",
};

/** Free-text chat with a task's agent. While the task is running a message steers
 *  it live; while it is blocked, a message *revives* it — the loop re-spawns the
 *  agent in the kept worktree with the whole conversation folded into its prompt,
 *  so the operator can workshop a failure back to green. A done task is terminal:
 *  the composer is disabled (restart to re-run). */
export function TaskChat({ task }: { task: Task }) {
  const { data: messages, isLoading } = useChat(task.id);
  const post = usePostChat(task.id);
  const [text, setText] = useState("");
  const [delivery, setDelivery] = useState<ChatDelivery | null>(null);
  const endRef = useRef<HTMLDivElement>(null);

  const isDone = task.status === "done";
  const postErr = post.error instanceof ApiError ? post.error.message : null;

  // Keep the latest message in view as the conversation grows / streams in.
  useEffect(() => {
    endRef.current?.scrollIntoView({ block: "end" });
  }, [messages?.length]);

  const send = () => {
    const t = text.trim();
    if (!t || post.isPending) return;
    post.mutate(t, {
      onSuccess: (res) => {
        setText("");
        setDelivery(res.delivery);
      },
    });
  };

  const placeholder = isDone
    ? "Task is done — restart it to re-run"
    : task.status === "blocked"
      ? "Tell the agent what to fix, then send to retry…"
      : "Message the agent to steer it…";

  return (
    <div className="flex flex-col gap-2">
      <div className="rounded-md border border-border bg-surface">
        <ScrollArea className="max-h-72">
          <div className="space-y-2 p-3">
            {isLoading && (
              <p className="text-[11px] text-muted-foreground">Loading conversation…</p>
            )}
            {!isLoading && (messages?.length ?? 0) === 0 && (
              <div className="flex flex-col items-center gap-1 py-6 text-center">
                <MessagesSquare className="size-5 text-muted-foreground/60" />
                <p className="text-[11px] text-muted-foreground">
                  No messages yet. {task.status === "blocked"
                    ? "Send guidance to revive this task."
                    : "Say something to the agent."}
                </p>
              </div>
            )}
            {messages?.map((m, i) => (
              <Bubble key={`${m.at}-${i}`} role={m.role} text={m.text} at={m.at} />
            ))}
            <div ref={endRef} />
          </div>
        </ScrollArea>
      </div>

      {delivery && !postErr && (
        <p className="text-[11px] text-muted-foreground">{DELIVERY_NOTE[delivery]}</p>
      )}
      {postErr && <p className="text-[11px] text-status-blocked">{postErr}</p>}

      <div className="flex items-end gap-2">
        <textarea
          value={text}
          disabled={isDone || post.isPending}
          onChange={(e) => setText(e.target.value)}
          onKeyDown={(e) => {
            // Enter sends; Shift+Enter inserts a newline.
            if (e.key === "Enter" && !e.shiftKey) {
              e.preventDefault();
              send();
            }
          }}
          placeholder={placeholder}
          rows={2}
          className={cn(
            "flex-1 resize-none rounded-md border border-border bg-surface px-2.5 py-1.5 text-xs",
            "placeholder:text-muted-foreground/60 focus:border-border-strong focus:outline-none",
            "disabled:cursor-not-allowed disabled:opacity-60",
          )}
        />
        <Button
          size="sm"
          disabled={isDone || post.isPending || !text.trim()}
          onClick={send}
          title={isDone ? "Task is done — restart to re-run" : "Send (Enter)"}
        >
          <Send /> Send
        </Button>
      </div>
    </div>
  );
}

/** One message bubble: operator messages right-aligned/accented, agent replies
 *  left-aligned/muted, each with a relative timestamp. */
function Bubble({
  role,
  text,
  at,
}: {
  role: "user" | "agent";
  text: string;
  at: string;
}) {
  const isUser = role === "user";
  return (
    <div className={cn("flex flex-col gap-0.5", isUser ? "items-end" : "items-start")}>
      <div
        className={cn(
          "max-w-[85%] whitespace-pre-wrap rounded-lg px-2.5 py-1.5 text-xs leading-snug",
          isUser
            ? "bg-primary text-primary-foreground"
            : "bg-muted text-foreground",
        )}
      >
        {text}
      </div>
      <span className="px-1 text-[10px] text-muted-foreground/70">
        {isUser ? "you" : "agent"} · {relativeTime(at)}
      </span>
    </div>
  );
}
