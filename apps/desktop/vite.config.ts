import { defineConfig } from "vite";

export default defineConfig({
  server: { port: 1420, strictPort: true, watch: { ignored: ["**/src-tauri/**"] } },
  clearScreen: false,
});
