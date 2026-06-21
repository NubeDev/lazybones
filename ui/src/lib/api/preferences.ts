import { request } from "./client";
import type { Preferences, PreferencesDraft } from "@/types/preferences";

/** `GET /settings/preferences` — the current preferences, or their defaults if
 *  the operator has never saved any (open read). */
export function getPreferences(signal?: AbortSignal): Promise<Preferences> {
  return request<Preferences>("/settings/preferences", { signal });
}

/** `PUT /settings/preferences` — replace the preferences record. Requires
 *  `Author`. An omitted field clears that preference. */
export function updatePreferences(draft: PreferencesDraft): Promise<Preferences> {
  return request<Preferences>("/settings/preferences", {
    method: "PUT",
    auth: true,
    body: draft,
  });
}
