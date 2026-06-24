import type { ExtensionRecord } from "@/lib/api/extensions";

/** Human-facing copy for each known host capability a guest may request.
 *  Mirrors the grant set in the design (§3.3). Unknown caps still render — they
 *  just fall back to the raw id with a neutral blurb. */
export const CAP_INFO: Record<string, { label: string; blurb: string }> = {
  log: {
    label: "Log",
    blurb: "Write structured log lines into the daemon. Always safe.",
  },
  "store-read": {
    label: "Store read",
    blurb: "Read a narrow, projected view of tasks and runs.",
  },
  "store-write": {
    label: "Store write",
    blurb: "Mutate tasks and runs. Deferred in v1 — grant with care.",
  },
  "http-fetch": {
    label: "HTTP fetch",
    blurb: "Make outbound HTTP requests to an allowlist of hosts.",
  },
  "secrets-read": {
    label: "Secrets read",
    blurb: "Read named secrets, decrypted by the host. High trust.",
  },
  kv: {
    label: "Key/value",
    blurb: "Per-extension namespaced scratch storage.",
  },
  "emit-event": {
    label: "Emit event",
    blurb: "Append an extension-namespaced event (cycle-guarded).",
  },
};

export function capLabel(cap: string): string {
  return CAP_INFO[cap]?.label ?? cap;
}

/** Capability combinations that are individually fine but dangerous together —
 *  classically `secrets-read` + `http-fetch` is an exfiltration pair (read a
 *  secret, POST it out; design §3.3). The grant UI flags these louder than any
 *  single cap. Returns one warning per matched pair present in `caps`. */
export function dangerousPairs(caps: string[]): string[] {
  const has = (c: string) => caps.includes(c);
  const warnings: string[] = [];
  if (has("secrets-read") && has("http-fetch")) {
    warnings.push(
      "Secrets read + HTTP fetch lets the extension read a secret and send it to an allowlisted host. Grant both only to a trusted extension.",
    );
  }
  if (has("store-write") && has("http-fetch")) {
    warnings.push(
      "Store write + HTTP fetch lets the extension copy task data out over the network.",
    );
  }
  return warnings;
}

/** The install provenance, rendered for the card. The daemon serialises
 *  `ExtensionSource` either as the string `"upload"` or as `{ url|registry: … }`. */
export function sourceLabel(source: ExtensionRecord["source"]): string {
  if (typeof source === "string") return source;
  if ("url" in source) return `url: ${source.url}`;
  if ("registry" in source) return `registry: ${source.registry}`;
  return "unknown";
}
