import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

// `base` pins every asset reference to the origin path the Rust server
// serves them from (/static/, docs/DESIGN.md §6). Output names are fixed
// (no hashes) because the binary embeds the files by name; cache busting is
// the server's short max-age, matching static/app.css.
export default defineConfig({
  base: "/static/",
  plugins: [react(), tailwindcss()],
  server: {
    proxy: {
      // The desk talks to the Rust service; during `pnpm dev` the calls are
      // forwarded to the locally running server so no CORS path exists.
      "/api": "http://127.0.0.1:8080",
      "/receipt": "http://127.0.0.1:8080",
    },
  },
  build: {
    sourcemap: false,
    rollupOptions: {
      output: {
        // One JS file and one CSS file, flat, stable names — the exact set
        // src/web/assets.rs embeds. A single entry never chunks, and a fixed
        // chunk name keeps it honest if one ever appears.
        entryFileNames: "spa.js",
        chunkFileNames: "spa.js",
        assetFileNames: (info) =>
          (info.names ?? []).some((n) => n.endsWith(".css")) ? "spa.css" : "[name][extname]",
      },
    },
  },
});
