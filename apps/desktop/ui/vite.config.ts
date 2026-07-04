import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

export default defineConfig({
  plugins: [react(), tailwindcss()],
  // Tauri expects a fixed dev port and no screen clearing over its own output.
  clearScreen: false,
  server: { port: 5173, strictPort: true },
});
