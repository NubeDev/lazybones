import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { getSyncStatus, pullSync, pushSync } from "@/lib/api/content-sync";
import { extToast } from "@/lib/ext/toast";
import type { JobReport, SyncStatus } from "@/types/content-sync";

/** Poll the content-sync status. Drives the "out of sync — pull?" banner; the
 *  poll is the courier (no SSE for sync since it depends on a network fetch the
 *  daemon does lazily). Fails soft — an offline daemon just yields no data. */
export function useSyncStatus() {
  return useQuery({
    queryKey: ["content-sync-status"],
    queryFn: ({ signal }) => getSyncStatus(signal),
    // The remote-fetch the daemon does is the slow part; 30s keeps it cheap while
    // still catching "someone pushed from another machine" within half a minute.
    refetchInterval: 30_000,
    retry: false,
  });
}

/** Pull from the remote + import. On success, refetch status (and everything the
 *  import may have changed) and toast the summary. */
export function usePullSync() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: () => pullSync(),
    onSuccess: (report: JobReport) => {
      extToast.success("Pulled from sync", report.summary);
      invalidateAfterSync(qc);
    },
    onError: (e: Error) => extToast.error("Pull failed", e.message),
  });
}

/** Export + push to the remote. */
export function usePushSync() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: () => pushSync(),
    onSuccess: (report: JobReport) => {
      extToast.success("Pushed to sync", report.summary);
      qc.invalidateQueries({ queryKey: ["content-sync-status"] });
    },
    onError: (e: Error) => extToast.error("Push failed", e.message),
  });
}

/** A pull may have changed any synced entity; refetch the lot plus the status. */
function invalidateAfterSync(qc: ReturnType<typeof useQueryClient>) {
  for (const key of [
    "content-sync-status",
    "documents",
    "skills",
    "tasks",
    "templates",
    "workflows",
  ]) {
    qc.invalidateQueries({ queryKey: [key] });
  }
}

/** Whether a status should surface the "pull?" banner. */
export function isBehind(status: SyncStatus | undefined): boolean {
  return status?.state === "behind" || status?.state === "diverged";
}
