/** True when running inside the Tauri desktop shell (vs. a plain browser). */
export function isDesktop(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

/** localStorage key for the operator's chosen display timezone. */
const TZ_KEY = "lazybones-timezone";

/** The IANA timezone to render timestamps in, or `null` to follow the browser's
 *  own zone (the default). Persisted in localStorage and read on each format call
 *  so a change from Settings applies on the next render without a reload. */
export function getTimezone(): string | null {
  if (typeof localStorage === "undefined") return null;
  const tz = localStorage.getItem(TZ_KEY);
  return tz && tz.length > 0 ? tz : null;
}

/** Set (or clear, with an empty string) the display timezone. */
export function setTimezone(tz: string): void {
  if (typeof localStorage === "undefined") return;
  if (tz.length > 0) localStorage.setItem(TZ_KEY, tz);
  else localStorage.removeItem(TZ_KEY);
}

/** The timezone passed to `toLocaleString`. `undefined` means "follow the
 *  browser/OS zone" — `Intl` treats an explicit `undefined` `timeZone` as the
 *  local default. */
function displayTz(): string | undefined {
  return getTimezone() ?? undefined;
}

/** Format an RFC3339 timestamp as a short, human label in the display zone (no
 *  timezone suffix — the column is tight). */
export function shortTime(iso: string | null | undefined): string {
  if (!iso) return "—";
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso;
  return d.toLocaleString(undefined, {
    month: "short",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
    timeZone: displayTz(),
  });
}

/** The full timestamp (date, seconds, and timezone) in the display zone — for a
 *  hover tooltip alongside the compact [`shortTime`] label. */
export function fullTime(iso: string | null | undefined): string {
  if (!iso) return "—";
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso;
  return d.toLocaleString(undefined, {
    year: "numeric",
    month: "short",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
    timeZoneName: "short",
    timeZone: displayTz(),
  });
}

/** A compact elapsed label ("1h 12m", "45s") between two RFC3339 instants.
 *  Returns "—" if either end is missing or unparseable. When `end` is omitted
 *  the duration runs to now, so an in-flight task shows live elapsed time. */
export function duration(
  start: string | null | undefined,
  end?: string | null | undefined,
): string {
  if (!start) return "—";
  const from = new Date(start).getTime();
  const to = end ? new Date(end).getTime() : Date.now();
  if (Number.isNaN(from) || Number.isNaN(to) || to < from) return "—";
  const s = Math.round((to - from) / 1000);
  if (s < 60) return `${s}s`;
  const m = Math.floor(s / 60);
  if (m < 60) return s % 60 ? `${m}m ${s % 60}s` : `${m}m`;
  const h = Math.floor(m / 60);
  return m % 60 ? `${h}h ${m % 60}m` : `${h}h`;
}

/** A relative "3m ago" label, falling back to the absolute time. */
export function relativeTime(iso: string | null | undefined): string {
  if (!iso) return "—";
  const d = new Date(iso);
  const ms = d.getTime();
  if (Number.isNaN(ms)) return iso;
  const diff = Date.now() - ms;
  const s = Math.round(diff / 1000);
  if (s < 60) return `${s}s ago`;
  const m = Math.round(s / 60);
  if (m < 60) return `${m}m ago`;
  const h = Math.round(m / 60);
  if (h < 24) return `${h}h ago`;
  return shortTime(iso);
}
