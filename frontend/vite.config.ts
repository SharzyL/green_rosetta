import { resolve } from "node:path";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

// Read API URL from environment, default to localhost:8081
const apiUrl = process.env.VITE_API_URL || "http://localhost:8081";

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
      "^/media": {
        target: apiUrl,
        changeOrigin: true,
      },
    },
  },
  build: {
    outDir: "dist",
    sourcemap: false,
    minify: "terser",
    rollupOptions: {
      input: {
        main: resolve(__dirname, "index.html"),
        scrutinizer: resolve(__dirname, "scrutinizer/index.html"),
        outsideNow: resolve(__dirname, "404/index.html"),
      },
    },
  },
  optimizeDeps: {
    include: ["dashjs"],
  },
});
