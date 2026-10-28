import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

// Everything the webview loads is bundled here; the CSP in src-tauri/tauri.conf.json
// allows no remote origins (plan §13.5).
export default defineConfig({
  base: "./",
  plugins: [react(), tailwindcss()],
  clearScreen: false,
  server: { port: 5173, strictPort: true, host: "localhost" },
  build: {
    outDir: "dist",
    target: "es2022",
    // No inline <script>/<style>: keeps script-src/style-src at 'self'.
    assetsInlineLimit: 0,
    modulePreload: { polyfill: false },
  },
});
