import { describe, expect, test } from "bun:test";
import { resolve } from "node:path";
import { build } from "vite";

/**
 * A normal (non-lib) build of the asset module: Vite must emit the GeoJSON files as hashed assets and
 * rewrite the URLs. It only does so for the literal `new URL("...", import.meta.url)` form (ASM-501);
 * lib mode would inline everything and prove nothing.
 */
async function bundle(base: string) {
  const result = await build({
    configFile: false,
    logLevel: "silent",
    base,
    build: {
      write: false,
      rollupOptions: {
        input: resolve(import.meta.dirname, "basemapAssets.ts"),
        preserveEntrySignatures: "exports-only",
      },
    },
  });
  const outputs = (Array.isArray(result) ? result : [result]).flatMap((r) =>
    "output" in r ? r.output : [],
  );
  const assets = outputs.flatMap((o) => (o.type === "asset" ? [o.fileName] : []));
  const code = outputs.map((o) => (o.type === "chunk" ? o.code : "")).join("\n");
  return { assets, code };
}

describe("the basemap assets in a production build", () => {
  for (const base of ["/", "./"]) {
    test(`base "${base}": two hashed GeoJSON assets, referenced by the chunk, nothing inlined`, async () => {
      const { assets, code } = await bundle(base);
      const geo = assets.filter((a) => a.endsWith(".geojson")).sort();
      expect(geo).toHaveLength(2);
      expect(geo[0]).toMatch(/^assets\/ne_50m_admin0_lines-[\w-]+\.geojson$/);
      expect(geo[1]).toMatch(/^assets\/ne_50m_land-[\w-]+\.geojson$/);
      for (const file of geo) expect(code).toContain(file.slice("assets/".length));
      expect(code).not.toContain("data:");
      expect(code).not.toContain("features/map-view");
      expect(code).not.toContain("/assets/ne_50m_land.geojson"); // unhashed source name
    }, 60_000);
  }
});
