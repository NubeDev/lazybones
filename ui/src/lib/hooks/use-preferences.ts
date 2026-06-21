import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { getPreferences, updatePreferences } from "@/lib/api/preferences";
import { setTimezone } from "@/lib/utils/platform";
import type { Preferences, PreferencesDraft } from "@/types/preferences";

/** Read the single global user-preferences record. On success the timezone is
 *  mirrored into localStorage so the synchronous date formatters
 *  ([`shortTime`] et al.) pick it up without an async read. */
export function usePreferences() {
  return useQuery({
    queryKey: ["preferences"],
    queryFn: async ({ signal }) => {
      const prefs = await getPreferences(signal);
      setTimezone(prefs.timezone ?? "");
      return prefs;
    },
    retry: false,
  });
}

/** Save the user-preferences record (`PUT /settings/preferences`). Mirrors the
 *  saved timezone into localStorage so formatting updates immediately. */
export function useUpdatePreferences() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (draft: PreferencesDraft) => updatePreferences(draft),
    onSuccess: (prefs: Preferences) => {
      setTimezone(prefs.timezone ?? "");
      qc.setQueryData(["preferences"], prefs);
    },
  });
}
