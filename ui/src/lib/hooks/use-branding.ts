import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  createBranding,
  deleteBranding,
  getBranding,
  listBranding,
  updateBranding,
  type BrandingDraft,
} from "@/lib/api/branding";

/** Poll the brand-profile catalogue. */
export function useBrandingList() {
  return useQuery({
    queryKey: ["branding"],
    queryFn: ({ signal }) => listBranding(undefined, signal),
    refetchInterval: 10000,
  });
}

/** Fetch one brand profile. Disabled when `id` is absent. */
export function useBranding(id?: string) {
  return useQuery({
    queryKey: ["branding", id],
    queryFn: ({ signal }) => getBranding(id as string, signal),
    enabled: id != null,
  });
}

/** Author a new brand profile (`POST /branding`). */
export function useCreateBranding() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ id, draft }: { id: string; draft: BrandingDraft }) =>
      createBranding(id, draft),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["branding"] }),
  });
}

/** Edit a brand profile (`PUT /branding/:id`). */
export function useUpdateBranding() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ id, draft }: { id: string; draft: BrandingDraft }) =>
      updateBranding(id, draft),
    onSuccess: (_data, { id }) => {
      qc.invalidateQueries({ queryKey: ["branding"] });
      qc.invalidateQueries({ queryKey: ["branding", id] });
    },
  });
}

/** Delete a brand profile (`DELETE /branding/:id`). */
export function useDeleteBranding() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => deleteBranding(id),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["branding"] }),
  });
}
