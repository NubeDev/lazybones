import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  deleteExtension,
  disableExtension,
  enableExtension,
  getExtension,
  installFromUrl,
  listExtensions,
  setGrants,
  uploadExtension,
} from "@/lib/api/extensions";

/** Poll the installed-extension list. */
export function useExtensions() {
  return useQuery({
    queryKey: ["extensions"],
    queryFn: ({ signal }) => listExtensions(signal),
    refetchInterval: 8000,
  });
}

/** Fetch a single extension by id (`GET /extensions/:id`). */
export function useExtension(id?: string) {
  return useQuery({
    queryKey: ["extension", id],
    queryFn: ({ signal }) => getExtension(id as string, signal),
    enabled: id != null,
  });
}

/** Install from an uploaded `.wasm` component (`POST /extensions`). */
export function useUploadExtension() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ file, id }: { file: File; id?: string }) =>
      uploadExtension(file, id),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["extensions"] }),
  });
}

/** Install from a URL the daemon fetches (`POST /extensions` with `{ url }`). */
export function useInstallFromUrl() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ url, id }: { url: string; id?: string }) =>
      installFromUrl(url, id),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["extensions"] }),
  });
}

/** Uninstall an extension (`DELETE /extensions/:id`). */
export function useDeleteExtension() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => deleteExtension(id),
    onSuccess: (_data, id) => {
      qc.invalidateQueries({ queryKey: ["extensions"] });
      qc.invalidateQueries({ queryKey: ["extension", id] });
    },
  });
}

/** Flip an extension's enabled flag (`POST /extensions/:id/{enable,disable}`). */
export function useToggleExtension() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ id, enabled }: { id: string; enabled: boolean }) =>
      enabled ? enableExtension(id) : disableExtension(id),
    onSuccess: (_data, { id }) => {
      qc.invalidateQueries({ queryKey: ["extensions"] });
      qc.invalidateQueries({ queryKey: ["extension", id] });
    },
  });
}

/** Set the granted capability subset (`POST /extensions/:id/grants`). */
export function useSetGrants() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ id, grantedCaps }: { id: string; grantedCaps: string[] }) =>
      setGrants(id, grantedCaps),
    onSuccess: (_data, { id }) => {
      qc.invalidateQueries({ queryKey: ["extensions"] });
      qc.invalidateQueries({ queryKey: ["extension", id] });
    },
  });
}
