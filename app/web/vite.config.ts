import { defineConfig } from "vite";
import vue from "@vitejs/plugin-vue";

// Dev: `oijudger-cli serve <contest>` on :8080, then `npm run dev` here.
export default defineConfig({
  plugins: [vue()],
  clearScreen: false,
  server: {
    port: 5174,
    strictPort: true,
    proxy: { "/api": { target: "http://127.0.0.1:8080", changeOrigin: false } },
  },
  build: { target: "es2022", chunkSizeWarningLimit: 2500 },
});
