import { describe, expect, test } from "bun:test";
import { resolve } from "node:path";
import { build } from "vite";

/**
 * Builds the real app (the root vite.config.ts, minified, in memory) in production and in e2e mode and looks
 * at the emitted files (ASM-309, ASM-371). The e2e build is the control: if the marker were not found there,
 * the production assertion would pass vacuously.
 */
interface Built {
  code: string;
  files: string[];
  /** The entry chunk (loaded on every route) and the lazy dashboard chunk (the map). */
  entry: string;
  dashboard: string;
  /** The lazy templates chunk (WI-15; the page and the template browser). */
  templates: string;
  /** The lazy plugins chunk (WI-16; the page and plugin-status). */
  plugins: string;
  /** The lazy settings chunk (WI-17; the page, the setting editors and the desktop connection stub). */
  settings: string;
}

/**
 * Size budgets in characters of minified code (about bytes), recorded in ASM-510. Vite's own
 * `chunkSizeWarningLimit` (kB) in vite.config.ts is the dashboard budget, so the build prints no standing warning.
 */
const ENTRY_BUDGET = 400_000;
const DASHBOARD_BUDGET = 1_050_000;
const TEMPLATES_BUDGET = 26_000;
/** WI-16: measured 11,830 characters, 5% headroom (ASM-853, ASM-994). */
const PLUGINS_BUDGET = 12_500;
/** WI-17: measured 7,273 characters, 5% headroom (ASM-853, ASM-996); the editors land here because `@umbra/settings` and `@umbra/desktop` declare that only their CSS files have side effects. */
const SETTINGS_BUDGET = 7_700;
/** A MapLibre API name that survives minification: present in the map chunk only. */
const MAPLIBRE_MARKER = "queryRenderedFeatures";

async function bundle(mode: "production" | "e2e"): Promise<Built> {
  // `bun test` sets NODE_ENV=test, which would bundle React's development build and inflate the sizes.
  const previous = process.env.NODE_ENV;
  process.env.NODE_ENV = "production";
  const result = await build({
    root: resolve(import.meta.dirname, ".."),
    mode,
    logLevel: "silent",
    build: { write: false, minify: true, outDir: `dist-test-${mode}` },
  }).finally(() => {
    if (previous === undefined) delete process.env.NODE_ENV;
    else process.env.NODE_ENV = previous;
  });
  const outputs = (Array.isArray(result) ? result : [result]).flatMap((r) =>
    "output" in r ? r.output : [],
  );
  const chunks = outputs.flatMap((o) => (o.type === "chunk" ? [o] : []));
  const entry = chunks.find((c) => c.isEntry);
  const dashboard = chunks.find((c) => /DashboardPage/.test(c.fileName));
  const templates = chunks.find((c) => /TemplatesPage/.test(c.fileName));
  const plugins = chunks.find((c) => /PluginsPage/.test(c.fileName));
  const settings = chunks.find((c) => /SettingsPage/.test(c.fileName));
  if (!entry || !dashboard || !templates || !plugins || !settings)
    throw new Error("the app build has no entry, dashboard, templates, plugins or settings chunk");
  return {
    code: chunks.map((c) => c.code).join("\n"),
    files: outputs.map((o) => o.fileName),
    entry: entry.code,
    dashboard: dashboard.code,
    templates: templates.code,
    plugins: plugins.code,
    settings: settings.code,
  };
}

const built = {
  production: await bundle("production"),
  e2e: await bundle("e2e"),
};

describe("the e2e hook in the real app build", () => {
  test("the e2e build contains the hook", () => {
    expect(built.e2e.code).toContain("__UMBRA_E2E__");
  });
  test("the production build contains no trace of it", () => {
    expect(built.production.code.length).toBeGreaterThan(100_000); // the app and the map, not an empty bundle
    expect(built.production.code).not.toContain("__UMBRA_E2E__");
  });
  test("the e2e build reads injected navigation entries, the production build does not (ASM-992)", () => {
    expect(built.e2e.code).toContain("__UMBRA_E2E_NAV__");
    expect(built.production.code).not.toContain("__UMBRA_E2E_NAV__");
  });
  test.each(["production", "e2e"] as const)("the %s build emits both basemap assets", (mode) => {
    const files = built[mode].files;
    expect(files.some((f) => /ne_50m_land-.*\.geojson$/.test(f))).toBe(true);
    expect(files.some((f) => /ne_50m_admin0_lines-.*\.geojson$/.test(f))).toBe(true);
  });
  test.each(["production", "e2e"] as const)(
    "the %s entry chunk has no MapLibre and stays in budget",
    (mode) => {
      const { entry, dashboard } = built[mode];
      expect(dashboard).toContain(MAPLIBRE_MARKER); // the marker is real: the map chunk has it
      expect(entry).not.toContain(MAPLIBRE_MARKER);
      expect(entry.length).toBeLessThan(ENTRY_BUDGET);
    },
  );
  test("the dashboard chunk stays in budget", () => {
    expect(built.production.dashboard.length).toBeLessThan(DASHBOARD_BUDGET);
  });
  test("the dashboard is a separate chunk behind the router", () => {
    expect(built.production.files.filter((f) => /DashboardPage/.test(f)).length).toBeGreaterThan(0);
  });
});

describe("the templates chunk (WI-15)", () => {
  test("is a separate chunk behind the router, without MapLibre, and stays in budget", () => {
    expect(built.production.files.filter((f) => /TemplatesPage/.test(f)).length).toBeGreaterThan(0);
    expect(built.production.templates).not.toContain(MAPLIBRE_MARKER);
    expect(built.production.templates.length).toBeLessThan(TEMPLATES_BUDGET);
  });
});

describe("the plugins chunk (WI-16)", () => {
  test("is a separate chunk behind the router, without MapLibre, and stays in budget", () => {
    expect(built.production.files.filter((f) => /PluginsPage/.test(f)).length).toBeGreaterThan(0);
    expect(built.production.plugins).toBeDefined();
    expect(built.production.plugins).not.toContain(MAPLIBRE_MARKER);
    expect(built.production.plugins.length).toBeLessThan(PLUGINS_BUDGET);
  });
});

describe("the settings chunk (WI-17)", () => {
  test("is a separate chunk behind the router, without MapLibre, and stays in budget", () => {
    expect(built.production.files.filter((f) => /SettingsPage/.test(f)).length).toBeGreaterThan(0);
    expect(built.production.settings).not.toContain(MAPLIBRE_MARKER);
    expect(built.production.settings.length).toBeLessThan(SETTINGS_BUDGET);
  });
});
