import { useQuery } from "@tanstack/react-query";
import { runHistory } from "@/lib/api/runs";

/** Poll a run's transition log. */
export function useRunHistory(run: string | null) {
  return useQuery({
    queryKey: ["run", run],
    queryFn: ({ signal }) => runHistory(run!, signal),
    enabled: !!run,
    refetchInterval: 4000,
  });
}
