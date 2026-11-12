import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// The bridge serves `dist/` at http://127.0.0.1:8787. In development the Vite dev
// server proxies the WebSocket so both modes hit the same path.
export default defineConfig({
  plugins: [react()],
  build: { outDir: "dist", emptyOutDir: true },
  server: {
    port: 5174,
    proxy: {
      "/ws": { target: "ws://127.0.0.1:8787", ws: true, changeOrigin: true },
    },
  },
});
