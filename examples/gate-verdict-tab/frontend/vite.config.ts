import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import { federation } from "@module-federation/vite";

// The Module Federation remote that ships this extension's UI. It exposes ONE
// mount module (`frontend.exposed-module = "./mount"` in `lazybones.ext.toml`),
// which the lazybones host imports and calls once with an `ExtSdkHandle`.
//
// The `shared` block MUST match the host's shared-singleton contract (see the
// host `ui/vite.config.ts`): the remote does not bundle its own React or — most
// importantly — its own copy of `@lazybones/ext-sdk`, or it would register slots
// into a private, empty registry the host never reads.
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
      name: "gate_verdict_tab",
      filename: "remoteEntry.js",
      exposes: {
        "./mount": "./src/mount.tsx",
      },
      shared: sharedSingletons,
    }),
  ],
  // The MF runtime + remote-entry modules use top-level await.
  build: { target: "esnext" },
});
