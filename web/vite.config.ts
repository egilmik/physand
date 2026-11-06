import { defineConfig } from "vite";

export default defineConfig({
  // Relative asset paths so the build can be hosted from any sub-path.
  base: "./",
  server: { host: true },
  build: { target: "es2022" },
});
