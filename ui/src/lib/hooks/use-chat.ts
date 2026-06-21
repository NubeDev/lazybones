import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { listChat, postChat } from "@/lib/api/chat";

/** A task's conversation (oldest first). Keyed under `["chat", id]` so the live
 *  stream's `chat` listener (use-live-stream) can invalidate it on each event,
 *  and the polling backstop reconciles dropped frames. */
export function useChat(id: string | null) {
  return useQuery({
    queryKey: ["chat", id],
    queryFn: ({ signal }) => listChat(id!, signal),
    enabled: !!id,
    refetchInterval: 4000,
  });
}

/** Post a message to a task's agent (`POST /tasks/:id/chat`). Invalidates the
 *  conversation plus the task/workflow queries (a post can revive a blocked task,
 *  changing its status). */
export function usePostChat(id: string) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (text: string) => postChat(id, text),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ["chat", id] });
      qc.invalidateQueries({ queryKey: ["tasks"] });
      qc.invalidateQueries({ queryKey: ["task"] });
      qc.invalidateQueries({ queryKey: ["workflows"] });
      qc.invalidateQueries({ queryKey: ["workflow"] });
    },
  });
}
