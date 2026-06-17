import { useQuery } from "@tanstack/react-query";
import { checkHealth } from "@/lib/api/health";

/** Liveness of lazybonesd, polled fast for a responsive status dot. */
export function useHealth() {
  return useQuery({
    queryKey: ["health"],
    queryFn: ({ signal }) => checkHealth(signal),
    refetchInterval: 5000,
    retry: false,
  });
}
