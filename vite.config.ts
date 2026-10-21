import { defineConfig } from "vite";

// Tauri dev server on a fixed port so tauri.conf.json devUrl stays valid.
export default defineConfig({
  clearScreen: false,
  server: {
    port: 5173,
    strictPort: true,
  },
  envPrefix: ["VITE_", "TAURI_"],
  build: {
    target: "es2022",
  },
});
