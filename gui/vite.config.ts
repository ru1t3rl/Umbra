import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import react from "@vitejs/plugin-react";
import { type Connect, defineConfig, type Plugin } from "vite";

const ICON_PATH = "/umbra_icon.svg";
const ICON_FILE = resolve(import.meta.dirname, "../umbra_icon.svg");

/** Serves and emits the repository-root icon byte for byte; no copy is committed (ASM-321). */
function umbraIcon(): Plugin {
  const serve = (server: { middlewares: Connect.Server }): void => {
    server.middlewares.use((req, res, next) => {
      if (req.url?.split("?")[0] !== ICON_PATH) return next();
      res.setHeader("Content-Type", "image/svg+xml");
      res.end(readFileSync(ICON_FILE));
    });
  };
  return {
    name: "umbra-icon",
    configureServer: serve,
    configurePreviewServer: serve,
    generateBundle() {
      this.emitFile({ type: "asset", fileName: "umbra_icon.svg", source: readFileSync(ICON_FILE) });
    },
  };
}

// The engine proxy keeps the browser origin and Host (no changeOrigin) so the server's Origin and
// Host checks see loopback; ws: true forwards the stream upgrade.
const proxy = () => ({
  "/api/": { target: process.env.UMBRA_ENGINE_URL ?? "http://127.0.0.1:7878", ws: true },
});

export default defineConfig(({ mode }) => ({
  plugins: [react(), umbraIcon()],
  define: mode === "e2e" ? { "import.meta.env.VITE_UMBRA_E2E": JSON.stringify("1") } : {},
  server: { proxy: proxy() },
  preview: { proxy: proxy() },
  ...(mode === "e2e" ? { build: { outDir: "dist-e2e" } } : {}),
}));
