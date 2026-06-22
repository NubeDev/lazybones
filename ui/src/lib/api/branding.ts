import { request } from "./client";
import type { Branding, BrandColors, BrandFonts } from "@/types/branding";

/** The authored fields of a brand profile (everything but id + timestamps). */
export interface BrandingDraft {
  name: string;
  logo_asset_id?: string | null;
  colors: BrandColors;
  fonts: BrandFonts;
  header_text: string;
  footer_text: string;
}

/** `GET /branding` — all brand profiles (open read), optionally `?project=`. */
export function listBranding(project?: string, signal?: AbortSignal): Promise<Branding[]> {
  const q = project ? `?project=${encodeURIComponent(project)}` : "";
  return request<Branding[]>(`/branding${q}`, { signal });
}

/** `GET /branding/:id` — one brand profile; `404` if absent. */
export function getBranding(id: string, signal?: AbortSignal): Promise<Branding> {
  return request<Branding>(`/branding/${encodeURIComponent(id)}`, { signal });
}

/** `POST /branding` — author a brand profile. `409` if the id is taken. */
export function createBranding(id: string, draft: BrandingDraft): Promise<Branding> {
  return request<Branding>("/branding", {
    method: "POST",
    auth: true,
    body: { id, ...draft },
  });
}

/** `PUT /branding/:id` — overwrite a brand profile. `404` if unknown. */
export function updateBranding(id: string, draft: BrandingDraft): Promise<Branding> {
  return request<Branding>(`/branding/${encodeURIComponent(id)}`, {
    method: "PUT",
    auth: true,
    body: draft,
  });
}

/** `DELETE /branding/:id` — remove a brand profile. */
export function deleteBranding(id: string): Promise<{ deleted: boolean }> {
  return request<{ deleted: boolean }>(`/branding/${encodeURIComponent(id)}`, {
    method: "DELETE",
    auth: true,
  });
}
