import { resolve } from "node:path";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

// Where `/api` and `/stream` go in dev: the worker under `pnpm dev`, unless overridden.
const apiUrl = process.env.VITE_API_URL || "http://localhost:8787";

export default defineConfig({
  plugins: [react()],
  server: {
    port: 3000,
    open: true,
    proxy: {
      "^/api": {
        target: apiUrl,
        changeOrigin: true,
      },
      "^/stream": {
        target: apiUrl,
        changeOrigin: true,
      },
    },
  },
  build: {
    sourcemap: false,
    rollupOptions: {
      input: {
        main: resolve(import.meta.dirname, "index.html"),
        scrutinizer: resolve(import.meta.dirname, "scrutinizer/index.html"),
        outsideNow: resolve(import.meta.dirname, "404/index.html"),
      },
    },
  },
  optimizeDeps: {
    include: ["dashjs"],
  },
});
