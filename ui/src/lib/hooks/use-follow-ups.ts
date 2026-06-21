import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { getRunFollowUps, resolveFollowUp } from "@/lib/api/follow-ups";

/** A run's follow-ups. Lazy: only fetches when `run` is non-null (the Follow-ups
 *  tab passes null until it's opened). Polls so a newly-filed follow-up (the
 *  scheduler files these on a stuck task) appears without a manual refresh. */
export function useRunFollowUps(run: string | null, status?: string) {
  return useQuery({
    queryKey: ["follow-ups", run, status ?? "all"],
    queryFn: ({ signal }) => getRunFollowUps(run!, { status }, signal),
    enabled: !!run,
    refetchInterval: run ? 10_000 : false,
  });
}

/** Resolve one follow-up, then refresh the run's list. */
export function useResolveFollowUp(run: string | null) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => resolveFollowUp(id),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: ["follow-ups", run] });
    },
  });
}
