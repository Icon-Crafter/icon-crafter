import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

/**
 * Vite config for the Icon Crafter editor UI.
 *
 * The Tauri shell (apps/desktop/tauri.conf.json) expects a fixed dev server on
 * http://localhost:1420 and the production bundle in ../ui/dist. Do not change
 * either contract without coordinating with the desktop shell.
 */
export default defineConfig(async ({ command }) => {
  const isDebug = command === "serve";

  return {
    plugins: [react()],
    // Keep the terminal readable: Rust logs and Tauri build output share it.
    clearScreen: false,
    envPrefix: ["VITE_", "TAURI_ENV_"],
    server: {
      port: 1420,
      strictPort: true,
      // Tauri may run on a fixed host; failing loudly is better than drifting.
      watch: {
        ignored: ["**/src-tauri/**"],
      },
    },
    build: {
      outDir: "dist",
      emptyOutDir: true,
      target: "chrome105",
      sourcemap: isDebug,
      minify: isDebug ? false : "esbuild",
      // Keep a readable report for the milestone bundle-size check.
      reportCompressedSize: true,
      chunkSizeWarningLimit: 900,
    },
  };
});
