import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

export default defineConfig({
  plugins: [react(), tailwindcss()],
  // Relative asset paths: Tauri's embedded webview serves the bundle from a
  // custom protocol where absolute "/assets/..." paths fail to load (blank
  // page). "./" makes them resolve against the document. Works in dev too.
  base: "./",
  // Tauri expects a fixed dev port and no screen clearing over its own output.
  clearScreen: false,
  server: { port: 5173, strictPort: true },
});
