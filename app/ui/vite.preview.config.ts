import { defineConfig } from "vite";
import preact from "@preact/preset-vite";

// The window against recorded snapshots (preview/), for screenshots and
// review. Never part of the app's bundle.
export default defineConfig({
  plugins: [preact()],
  root: "preview",
  clearScreen: false,
  server: { port: 1430, strictPort: true, fs: { allow: ["..", "../../../spec"] } },
  build: { target: "es2022" },
});
