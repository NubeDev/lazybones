import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import { federation } from "@module-federation/vite";

// The Module Federation remote that ships this extension's UI. It exposes ONE
// mount module (`frontend.exposed-module = "./mount"` in `lazybones.ext.toml`),
// which the lazybones host imports and calls once with an `ExtSdkHandle`.
//
// The `shared` block MUST match the host's shared-singleton contract (the host
// `ui/vite.config.ts`): the remote does NOT bundle its own React or its own copy
// of `@lazybones/ext-sdk`, or it would register its route into a private, empty
// registry the host never reads. We DO bundle our own shadcn UI components — the
// host's `@/components/ui/*` are not shared singletons — but they use the host's
// served Tailwind tokens, so they render consistently with the host theme.
const sharedSingletons = {
  react: { singleton: true, requiredVersion: "^19.0.0" },
  "react-dom": { singleton: true, requiredVersion: "^19.0.0" },
  "react/jsx-runtime": { singleton: true },
  "@lazybones/ext-sdk": { singleton: true, requiredVersion: "^0.1.0" },
};

export default defineConfig({
  plugins: [
    react(),
    federation({
      name: "weather",
      filename: "remoteEntry.js",
      exposes: {
        "./mount": "./src/mount.tsx",
      },
      // The host imports `./mount` at runtime; it doesn't consume generated
      // `.d.ts` from the remote, so skip DTS generation (avoids a non-fatal
      // type-emit step that needs a full tsc project).
      dts: false,
      // Emit `mf-manifest.json` so the MF runtime registers this remote as a
      // module-type remote (loads `remoteEntry.js` as ESM). Without it the
      // runtime injects a classic <script> and the ESM entry throws
      // "Cannot use import statement outside a module".
      manifest: true,
      shared: sharedSingletons,
    }),
  ],
  // The MF runtime + remote-entry modules use top-level await.
  build: { target: "esnext" },
});
