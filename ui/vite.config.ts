import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import { federation } from "@module-federation/vite";
import path from "node:path";

// Tauri expects a fixed dev port and looks for the build output in `dist/`.
// The same build serves the browser target — there is no Tauri-only code path
// in the bundle; the desktop bridge is feature-detected at runtime.
const host = process.env.TAURI_DEV_HOST;

// The shared-singleton contract every federated remote binds against (design
// §4.1). `singleton: true` guarantees one instance app-wide so remotes do NOT
// bundle their own React/query-client and styling stays consistent. Remotes are
// registered at *runtime* from `GET /extensions?frontend=1` (see src/lib/ext),
// so the host declares no static `remotes` here.
const sharedSingletons = {
  react: { singleton: true, requiredVersion: "^19.0.0" },
  "react-dom": { singleton: true, requiredVersion: "^19.0.0" },
  "react/jsx-runtime": { singleton: true },
  "@tanstack/react-query": { singleton: true, requiredVersion: "^5.0.0" },
  // The host SDK — its slot registry + installed services are global state, so
  // it MUST be a singleton or remotes would each get a private, empty registry.
  "@lazybones/ext-sdk": { singleton: true, requiredVersion: "^0.1.0" },
  // Design-system / Radix primitives the host already ships, deduped so an
  // extension's dialog/tabs share the host's instance and theme.
  "@radix-ui/react-dialog": { singleton: true },
  "@radix-ui/react-dropdown-menu": { singleton: true },
  "@radix-ui/react-tabs": { singleton: true },
  "@radix-ui/react-tooltip": { singleton: true },
  "@radix-ui/react-scroll-area": { singleton: true },
  "@radix-ui/react-separator": { singleton: true },
  "@radix-ui/react-slot": { singleton: true },
};

export default defineConfig({
  plugins: [
    react(),
    tailwindcss(),
    federation({
      name: "lazybones_host",
      // No static remotes — they are registered dynamically at runtime via the
      // Module Federation runtime against the daemon proxy.
      remotes: {},
      shared: sharedSingletons,
    }),
  ],
  resolve: {
    // `@lazybones/ext-sdk` is intentionally NOT aliased — it resolves as a real
    // workspace package (node_modules symlink) so Module Federation can share it
    // as a singleton. Aliasing it to source would make MF bypass the shared
    // scope (every remote would get a private, empty slot registry).
    alias: { "@": path.resolve(__dirname, "./src") },
  },
  // The MF runtime + remote-entry modules use top-level await; target a baseline
  // that supports it (also fine for Tauri's Chromium/WebKit webviews).
  build: { target: "esnext" },
  // The MF plugin needs the dev server origin to build absolute shared-scope URLs.
  server: {
    port: 51840,
    strictPort: true,
    host: host || false,
    origin: host ? `http://${host}:51840` : "http://localhost:51840",
    hmr: host ? { protocol: "ws", host, port: 51841 } : undefined,
    watch: { ignored: ["**/src-tauri/**"] },
  },
  // Tauri uses Chromium on Windows and WebKit on macOS/Linux.
  clearScreen: false,
});
