import { describe, expect, test } from "bun:test";
import { resolve } from "node:path";
import react from "@vitejs/plugin-react";
import { build } from "vite";

/**
 * Builds the package entry the way the app does and looks at the emitted code (ASM-309). The e2e build
 * is the control: if the marker were not found there, the production assertion would pass vacuously.
 */
async function bundle(e2e: boolean): Promise<string> {
  const result = await build({
    configFile: false,
    logLevel: "silent",
    plugins: [react()],
    define: e2e ? { "import.meta.env.VITE_UMBRA_E2E": JSON.stringify("1") } : {},
    build: {
      write: false,
      minify: true,
      lib: { entry: resolve(import.meta.dirname, "index.ts"), formats: ["es"], fileName: "out" },
      rollupOptions: {
        // The basemap module is external: a lib build inlines assets, which would add ~1.2 MB of data URLs.
        external: ["maplibre-gl", "react", "react/jsx-runtime", "react-dom", /basemapAssets/],
      },
    },
  });
  const outputs = (Array.isArray(result) ? result : [result]).flatMap((r) =>
    "output" in r ? r.output : [],
  );
  return outputs.map((o) => (o.type === "chunk" ? o.code : "")).join("\n");
}

describe("the e2e hook in built output", () => {
  test("the e2e build contains the hook", async () => {
    expect(await bundle(true)).toContain("__UMBRA_E2E__");
  }, 60_000);
  test("the production build contains no trace of it", async () => {
    const code = await bundle(false);
    expect(code.length).toBeGreaterThan(1000); // the bundle is real, not empty
    expect(code).not.toContain("__UMBRA_E2E__");
  }, 60_000);
});
