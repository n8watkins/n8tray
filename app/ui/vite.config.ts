import { defineConfig } from "vite";
import preact from "@preact/preset-vite";

// The window is served from the app bundle; the dev server is only for `tauri dev`.
export default defineConfig({
  plugins: [preact()],
  clearScreen: false,
  server: { port: 1420, strictPort: true },
  build: { target: "es2022", outDir: "dist", emptyOutDir: true, sourcemap: false },
});
