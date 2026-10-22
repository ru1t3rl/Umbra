// Architecture rules (REQ-A-4, gui/CLAUDE.md "Layering rules", ADR 0043/0045). Run by `bun run check:deps`
// from the gui root (or from a fixture workspace: tiers and paths are read relative to process.cwd()).
// Every rule has a failing fixture in test/arch-fixtures, asserted by test/architecture.test.ts.
const fs = require("node:fs");
const path = require("node:path");

/** Tier of each `features/<name>` from its package.json `umbra.tier`; a missing tier is a load error. */
function readTiers(root) {
  const dir = path.join(root, "features");
  const tiers = { foundation: [], domain: [] };
  if (!fs.existsSync(dir)) return tiers;
  for (const name of fs.readdirSync(dir).sort()) {
    const manifest = path.join(dir, name, "package.json");
    if (!fs.existsSync(manifest)) continue;
    const tier = JSON.parse(fs.readFileSync(manifest, "utf8"))?.umbra?.tier;
    if (tier !== "foundation" && tier !== "domain") {
      throw new Error(`features/${name}/package.json: umbra.tier must be "foundation" or "domain"`);
    }
    tiers[tier].push(name);
  }
  return tiers;
}

/** Every workspace package: its directory, and the directories of the `@umbra/*` packages it declares. */
function readWorkspaceDeps(root) {
  const pkgs = [];
  for (const group of ["features", "feature-shells"]) {
    const dir = path.join(root, group);
    if (!fs.existsSync(dir)) continue;
    for (const name of fs.readdirSync(dir).sort()) {
      const manifest = path.join(dir, name, "package.json");
      if (!fs.existsSync(manifest)) continue;
      pkgs.push({ dir: `${group}/${name}`, json: JSON.parse(fs.readFileSync(manifest, "utf8")) });
    }
  }
  const dirOf = new Map(pkgs.map((p) => [p.json.name, p.dir]));
  return pkgs.map((p) => {
    const declared = {
      ...p.json.dependencies,
      ...p.json.devDependencies,
      ...p.json.peerDependencies,
    };
    const allowed = [p.dir, ...Object.keys(declared).map((d) => dirOf.get(d))].filter(Boolean);
    return { dir: p.dir, allowed };
  });
}

const tiers = readTiers(process.cwd());
const workspaceDeps = readWorkspaceDeps(process.cwd());
const alt = (names) => (names.length > 0 ? names.join("|") : "(?!)");
const NODE_MODULES = "(^|/)node_modules/";
const ENTRY =
  "^features/[^/]+/src/(index\\.ts|styles\\.css)$|^feature-shells/[^/]+/src/index\\.ts$";
const lib = (name) => `${NODE_MODULES}${name.replace("/", "\\/")}/`;

const rule = (name, comment, from, to) => ({ name, comment, severity: "error", from, to });

/** @type {import('dependency-cruiser').IConfiguration} */
module.exports = {
  forbidden: [
    rule(
      "no-src-import",
      "Nothing imports the app (src/).",
      { pathNot: "^src/" },
      { path: "^src/" },
    ),
    rule(
      "shell-imports-features-only",
      "Shells import features only, never the app or tooling.",
      { path: "^feature-shells/" },
      { path: "^(src|test|e2e|scripts)/" },
    ),
    rule(
      "shell-not-shell",
      "A shell never imports another shell.",
      { path: "^feature-shells/([^/]+)/" },
      { path: "^feature-shells/", pathNot: "^feature-shells/$1/" },
    ),
    rule(
      "feature-not-shell-or-src",
      "Features never import shells or the app.",
      { path: "^features/" },
      { path: "^(feature-shells|src|test|e2e|scripts)/" },
    ),
    rule(
      "foundation-only-foundation",
      "A foundation feature imports only foundation features.",
      { path: `^features/(${alt(tiers.foundation)})/` },
      { path: `^features/(${alt(tiers.domain)})/` },
    ),
    rule(
      "domain-not-domain",
      "A domain feature never imports another domain feature; shells compose them.",
      { path: `^features/(${alt(tiers.domain)})/` },
      { path: `^features/(${alt(tiers.domain)})/`, pathNot: "^features/$1/" },
    ),
    rule(
      "no-deep-import",
      "Cross-package imports go through src/index.ts (or a declared src/styles.css).",
      { path: "^(features|feature-shells)/([^/]+)/" },
      { path: "^(features|feature-shells)/", pathNot: `^$1/$2/|${ENTRY}` },
    ),
    rule(
      "no-deep-import",
      "The app, tests and e2e import packages only through their public entry.",
      { path: "^(src|test|e2e|scripts)/" },
      { path: "^(features|feature-shells)/", pathNot: ENTRY },
    ),
    rule(
      "no-deep-import",
      "A package subpath other than ./styles.css is not exported.",
      {},
      { couldNotResolve: true, path: "^@umbra/[^/]+/(?!styles\\.css$)" },
    ),
    ...workspaceDeps.map((p) =>
      rule(
        "no-undeclared-workspace-dep",
        "A package imports only workspace packages declared in its package.json.",
        { path: `^${p.dir}/` },
        {
          path: "^(features|feature-shells)/",
          pathNot: p.allowed.map((d) => `^${d}/`).join("|"),
        },
      ),
    ),
    rule("no-circular", "No cyclic dependencies.", {}, { circular: true }),
    rule(
      "no-engine-source",
      "REQ-A-4: nothing resolves outside gui/ (no ../crates, ../api, ../examples); only node_modules.",
      {},
      { path: "^\\.\\./" },
    ),
    rule(
      "no-generated-from-outside",
      "Only the owning package imports its src/generated/.",
      { path: "^(features/[^/]+|feature-shells|src|test|e2e|scripts)/" },
      { path: "^features/[^/]+/src/generated/", pathNot: "^$1/" },
    ),
    rule(
      "maplibre-only-in-map-view",
      "maplibre-gl is confined to @umbra/map-view (F-8).",
      { pathNot: "^features/map-view/" },
      { path: lib("maplibre-gl") },
    ),
    rule(
      "zustand-only-in-engine-stream",
      "zustand is confined to @umbra/engine-stream (F-5).",
      { pathNot: "^features/engine-stream/" },
      { path: lib("zustand") },
    ),
    rule(
      "react-query-only-in-data",
      "@tanstack/react-query is confined to @umbra/data (F-5).",
      { pathNot: "^features/data/" },
      { path: lib("@tanstack/react-query") },
    ),
    rule(
      "router-only-in-src",
      "react-router is confined to the app (src/); shells export route objects.",
      { pathNot: "^src/" },
      { path: `${NODE_MODULES}react-router(-dom)?/` },
    ),
    rule(
      "not-to-unresolvable",
      "Every import must resolve.",
      {},
      { couldNotResolve: true, pathNot: "^bun:" },
    ),
    rule(
      "no-non-package-json",
      "Every npm import is a declared dependency.",
      {},
      { dependencyTypes: ["npm-no-pkg", "npm-unknown"] },
    ),
  ],
  options: {
    doNotFollow: { path: "node_modules" },
    exclude: {
      path: "(^|/)(dist|dist-e2e|\\.tsbuild|test-results|playwright-report|reference|arch-fixtures)/",
    },
    tsPreCompilationDeps: true,
    combinedDependencies: false,
    enhancedResolveOptions: {
      exportsFields: ["exports"],
      conditionNames: ["import", "require", "node", "default"],
      extensions: [".ts", ".d.ts", ".tsx", ".js", ".jsx", ".mjs", ".cjs", ".json", ".css"],
    },
    tsConfig: { fileName: "tsconfig.base.json" },
    reporterOptions: { text: { highlightFocused: true } },
  },
};
