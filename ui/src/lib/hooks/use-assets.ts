import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { deleteAsset, listAssets, uploadAsset } from "@/lib/api/assets";

/** Poll the asset library. */
export function useAssets() {
  return useQuery({
    queryKey: ["assets"],
    queryFn: ({ signal }) => listAssets(undefined, signal),
    refetchInterval: 10000,
  });
}

/** Upload a file to the asset server (`POST /assets`). */
export function useUploadAsset() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ file, project }: { file: File; project?: string }) =>
      uploadAsset(file, project),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["assets"] }),
  });
}

/** Delete an asset (`DELETE /assets/:id`). */
export function useDeleteAsset() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => deleteAsset(id),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["assets"] }),
  });
}
