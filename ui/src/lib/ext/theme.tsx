import { useEffect } from "react";
import { createObservable } from "@lazybones/ext-sdk";
import type { ExtTheme, ExtThemeSnapshot, ThemeMode } from "@lazybones/ext-sdk";
import { useTheme } from "@/lib/theme/theme-provider";

/** The design-system CSS custom properties exposed to remotes. Remotes should
 *  prefer the shared Tailwind/Radix classes; these tokens are the escape hatch
 *  for canvas/SVG/inline-style surfaces (design §4.1 "theme tokens"). */
const TOKEN_NAMES = [
  "--background",
  "--foreground",
  "--surface",
  "--muted",
  "--muted-foreground",
  "--border",
  "--border-strong",
  "--accent",
  "--accent-foreground",
  "--accent-soft",
  "--ring",
  "--radius",
  "--status-pending",
  "--status-ready",
  "--status-running",
  "--status-gating",
  "--status-done",
  "--status-blocked",
];

function readTokens(): Record<string, string> {
  const out: Record<string, string> = {};
  if (typeof document === "undefined") return out;
  const style = getComputedStyle(document.documentElement);
  for (const name of TOKEN_NAMES) {
    const value = style.getPropertyValue(name).trim();
    if (value) out[name] = value;
  }
  return out;
}

function snapshot(mode: ThemeMode): ExtThemeSnapshot {
  return { mode, tokens: readTokens() };
}

const store = createObservable<ExtThemeSnapshot>(
  snapshot(
    typeof document !== "undefined" && document.documentElement.classList.contains("dark")
      ? "dark"
      : "light",
  ),
);

/** The {@link ExtTheme} the host installs into the SDK. */
export const extTheme: ExtTheme = {
  current: () => store.get(),
  subscribe: (handler) => {
    const emit = () => handler(store.get());
    return store.subscribe(emit);
  },
};

/** Bridges the host {@link useTheme} into the extension theme store. Mounted
 *  inside `ThemeProvider` so it recomputes the token snapshot whenever the
 *  operator flips dark/light. Renders nothing. */
export function ExtThemeBridge() {
  const { theme } = useTheme();
  useEffect(() => {
    // Defer so the `dark` class is already applied to <html> before we read
    // resolved token values.
    const id = requestAnimationFrame(() => store.set(snapshot(theme)));
    return () => cancelAnimationFrame(id);
  }, [theme]);
  return null;
}
