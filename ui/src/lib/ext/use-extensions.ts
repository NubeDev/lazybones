import { useEffect, useRef, useState } from "react";
import { installExtensionHost } from "./host-services";
import { loadFrontendExtensions, type RemoteLoadResult } from "./loader";

/** Installs the SDK host services (synchronously, before any child renders) and
 *  loads the enabled frontend remotes once on boot. Returns the per-remote load
 *  results so a future extensions-management view can surface failures (design
 *  §4.3 version/trust mismatches are reported, not silently dropped). */
export function useFrontendExtensions(): {
  results: RemoteLoadResult[];
  loading: boolean;
} {
  // Install on first render (a ref guard makes it run exactly once, before the
  // boot effect and before remotes mount).
  const installed = useRef(false);
  if (!installed.current) {
    installExtensionHost();
    installed.current = true;
  }

  const [results, setResults] = useState<RemoteLoadResult[]>([]);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    let active = true;
    loadFrontendExtensions()
      .then((r) => {
        if (active) setResults(r);
      })
      .finally(() => {
        if (active) setLoading(false);
      });
    return () => {
      active = false;
    };
  }, []);

  return { results, loading };
}
