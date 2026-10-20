import { describe, expect, setDefaultTimeout, test } from "bun:test";
import {
  cpSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  readdirSync,
  readFileSync,
  rmSync,
  symlinkSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { findPlainRequestImports } from "../scripts/check-plain-requests.ts";

setDefaultTimeout(20000);

// Built from parts so the plain-request scan does not match this file.
const DYNAMIC_NAME = `import("@umbra/${"data"}")`;
const GUI = resolve(import.meta.dir, "..");
const FIXTURES = join(import.meta.dir, "arch-fixtures");
const DEPCRUISE = join(GUI, "node_modules/dependency-cruiser/bin/dependency-cruise.mjs");
const STUB_LIBS = ["maplibre-gl", "zustand", "@tanstack/react-query", "react-router", "left-pad"];

interface Violation {
  rule: string;
  from: string;
  to: string;
}

/** Builds `_base` + the case overlay in a temp dir (`<tmp>/gui`, siblings from `outside/`), runs the real config. */
function cruise(fixture: string | null): Violation[] {
  const tmp = mkdtempSync(join(tmpdir(), "umbra-arch-"));
  try {
    const root = join(tmp, "gui");
    cpSync(join(FIXTURES, "_base"), root, { recursive: true });
    if (fixture !== null) {
      const dir = join(FIXTURES, fixture);
      if (existsSync(join(dir, "gui"))) cpSync(join(dir, "gui"), root, { recursive: true });
      if (existsSync(join(dir, "outside"))) cpSync(join(dir, "outside"), tmp, { recursive: true });
    }
    // Workspace links, as `bun install` makes them.
    for (const group of ["features", "feature-shells"]) {
      for (const name of readdirSync(join(root, group))) {
        const manifest = JSON.parse(readFileSync(join(root, group, name, "package.json"), "utf8"));
        const link = join(root, "node_modules", manifest.name);
        mkdirSync(dirname(link), { recursive: true });
        symlinkSync(join(root, group, name), link, "dir");
      }
    }
    for (const lib of STUB_LIBS) {
      const dir = join(root, "node_modules", lib);
      mkdirSync(dir, { recursive: true });
      writeFileSync(
        join(dir, "package.json"),
        JSON.stringify({ name: lib, version: "1.0.0", main: "index.js" }),
      );
      writeFileSync(join(dir, "index.js"), "module.exports = {};\n");
    }
    const run = Bun.spawnSync(
      [
        "node",
        DEPCRUISE,
        "--config",
        join(GUI, ".dependency-cruiser.cjs"),
        "--output-type",
        "json",
        ".",
      ],
      { cwd: root, stdout: "pipe", stderr: "pipe" },
    );
    const out = run.stdout.toString();
    if (out.trim() === "")
      throw new Error(`depcruise produced no output: ${run.stderr.toString()}`);
    const result = JSON.parse(out);
    return result.summary.violations.map(
      (v: { rule: { name: string }; from: string; to: string }) => ({
        rule: v.rule.name,
        from: v.from,
        to: v.to,
      }),
    );
  } finally {
    rmSync(tmp, { recursive: true, force: true });
  }
}

/** fixture directory -> the rule it must trigger. */
const CASES: Record<string, string> = {
  "no-src-import": "no-src-import",
  "shell-imports-features-only": "shell-imports-features-only",
  "shell-not-shell": "shell-not-shell",
  "feature-not-shell-or-src": "feature-not-shell-or-src",
  "foundation-only-foundation": "foundation-only-foundation",
  "domain-not-domain": "domain-not-domain",
  "deep-import-relative": "no-deep-import",
  "deep-import-specifier": "no-deep-import",
  "deep-import-app": "no-deep-import",
  "feature-imports-tooling": "feature-not-shell-or-src",
  "undeclared-workspace-dep": "no-undeclared-workspace-dep",
  "no-circular": "no-circular",
  "no-engine-source-crates": "no-engine-source",
  "no-engine-source-api-json": "no-engine-source",
  "no-generated-from-outside": "no-generated-from-outside",
  "maplibre-only-in-map-view": "maplibre-only-in-map-view",
  "zustand-only-in-engine-stream": "zustand-only-in-engine-stream",
  "react-query-only-in-data": "react-query-only-in-data",
  "router-only-in-src": "router-only-in-src",
  "not-to-unresolvable": "not-to-unresolvable",
  "no-non-package-json": "no-non-package-json",
};

describe("dependency-cruiser architecture rules", () => {
  test("the clean fixture workspace has no violations", () => {
    expect(cruise(null)).toEqual([]);
  });

  for (const [fixture, rule] of Object.entries(CASES)) {
    test(`${fixture} is rejected by ${rule}`, () => {
      const rules = cruise(fixture).map((v) => v.rule);
      expect(rules).toContain(rule);
    });
  }

  test("engine source import is rejected (REQ-A-4: ../../crates and ../api/openapi.json)", () => {
    const crates = cruise("no-engine-source-crates").filter((v) => v.rule === "no-engine-source");
    const spec = cruise("no-engine-source-api-json").filter((v) => v.rule === "no-engine-source");
    expect(crates.map((v) => v.to)).toEqual(["../crates/umbra-core/lib.ts"]);
    expect(spec.map((v) => v.to)).toEqual(["../api/openapi.json"]);
  });

  test("every rule in the config has a failing fixture", () => {
    const config = require(join(GUI, ".dependency-cruiser.cjs")) as {
      forbidden: { name: string }[];
    };
    const names = new Set(config.forbidden.map((r) => r.name));
    expect([...names].sort()).toEqual([...new Set(Object.values(CASES))].sort());
  });

  test("a feature without umbra.tier fails the config load", () => {
    const tmp = mkdtempSync(join(tmpdir(), "umbra-arch-tier-"));
    try {
      mkdirSync(join(tmp, "features/x"), { recursive: true });
      writeFileSync(join(tmp, "features/x/package.json"), JSON.stringify({ name: "@umbra/x" }));
      const run = Bun.spawnSync(
        ["node", "-e", `require(${JSON.stringify(join(GUI, ".dependency-cruiser.cjs"))})`],
        { cwd: tmp, stdout: "pipe", stderr: "pipe" },
      );
      expect(run.exitCode).not.toBe(0);
      expect(run.stderr.toString()).toContain("umbra.tier");
    } finally {
      rmSync(tmp, { recursive: true, force: true });
    }
  });

  test("the real tree passes check:deps", () => {
    const run = Bun.spawnSync(["node", DEPCRUISE, "--config", ".dependency-cruiser.cjs", "."], {
      cwd: GUI,
      stdout: "pipe",
      stderr: "pipe",
    });
    expect(run.stdout.toString() + run.stderr.toString()).toContain(
      "no dependency violations found",
    );
    expect(run.exitCode).toBe(0);
  });
});

describe("plain request functions (ADR 0045)", () => {
  test("plain request functions are not imported by features or shells", () => {
    expect(findPlainRequestImports(GUI)).toEqual([]);
  });

  test("plain request imports are reported in features, shells and the app, hooks and types are not", () => {
    const found = findPlainRequestImports(join(FIXTURES, "plain-request/gui"));
    expect(found).toEqual([
      { file: "feature-shells/dashboard/src/index.ts", name: "* as data" },
      { file: "features/map-view/src/dynamic.ts", name: DYNAMIC_NAME },
      { file: "features/map-view/src/index.ts", name: "getHealth" },
      { file: "features/map-view/src/reexport.ts", name: "getHealth" },
      { file: "features/map-view/src/swallow.ts", name: "getHealth" },
      { file: "src/main.ts", name: "getHealth" },
    ]);
  });
});
