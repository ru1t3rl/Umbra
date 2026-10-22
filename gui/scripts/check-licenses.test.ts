import { afterAll, describe, expect, test } from "bun:test";
import { spawnSync } from "node:child_process";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const script = join(import.meta.dir, "check-licenses.ts");
const dirs: string[] = [];

interface Pkg {
  name: string;
  license?: string;
  dependencies?: Record<string, string>;
  optionalDependencies?: Record<string, string>;
}

function fixture(
  root: Record<string, unknown>,
  installed: Pkg[],
  members: Record<string, Record<string, unknown>> = {},
): string {
  const dir = mkdtempSync(join(tmpdir(), "umbra-lic-"));
  dirs.push(dir);
  writeFileSync(join(dir, "package.json"), JSON.stringify({ name: "app", private: true, ...root }));
  for (const [rel, pkg] of Object.entries(members)) {
    mkdirSync(join(dir, rel), { recursive: true });
    writeFileSync(join(dir, rel, "package.json"), JSON.stringify(pkg));
  }
  for (const p of installed) {
    const d = join(dir, "node_modules", p.name);
    mkdirSync(d, { recursive: true });
    writeFileSync(join(d, "package.json"), JSON.stringify({ version: "1.0.0", ...p }));
  }
  return dir;
}

function run(dir: string) {
  const r = spawnSync("bun", [script, "--root", dir], { encoding: "utf8" });
  return { code: r.status, out: `${r.stdout}${r.stderr}` };
}

afterAll(() => {
  for (const d of dirs) rmSync(d, { recursive: true, force: true });
});

describe("check-licenses", () => {
  test("an allowed runtime licence passes", () => {
    const d = fixture({ dependencies: { a: "1.0.0" } }, [{ name: "a", license: "MIT" }]);
    expect(run(d).code).toBe(0);
  });
  test("GPL-3.0 fails", () => {
    const d = fixture({ dependencies: { a: "1.0.0" } }, [{ name: "a", license: "GPL-3.0" }]);
    const r = run(d);
    expect(r.code).toBe(1);
    expect(r.out).toContain("GPL-3.0");
  });
  test("a dev-only MPL-2.0 passes", () => {
    const d = fixture({ devDependencies: { a: "1.0.0" } }, [{ name: "a", license: "MPL-2.0" }]);
    expect(run(d).code).toBe(0);
  });
  test("a runtime MPL-2.0 fails", () => {
    const d = fixture({ dependencies: { a: "1.0.0" } }, [{ name: "a", license: "MPL-2.0" }]);
    expect(run(d).code).toBe(1);
  });
  test("a transitive runtime dependency is runtime", () => {
    const d = fixture({ dependencies: { a: "1.0.0" } }, [
      { name: "a", license: "MIT", dependencies: { b: "1.0.0" } },
      { name: "b", license: "MPL-2.0" },
    ]);
    expect(run(d).code).toBe(1);
  });
  test("a missing licence fails", () => {
    const d = fixture({ dependencies: { a: "1.0.0" } }, [{ name: "a" }]);
    expect(run(d).code).toBe(1);
  });
  test("an SPDX OR expression passes if one side is allowed", () => {
    const d = fixture({ dependencies: { a: "1.0.0" } }, [
      { name: "a", license: "(MIT OR GPL-3.0)" },
    ]);
    expect(run(d).code).toBe(0);
  });
  test("an AND expression needs both sides", () => {
    const d = fixture({ dependencies: { a: "1.0.0" } }, [
      { name: "a", license: "(MIT AND GPL-3.0)" },
    ]);
    expect(run(d).code).toBe(1);
  });
  test("a declared dependency that is not installed fails", () => {
    const d = fixture({ dependencies: { a: "1.0.0" } }, []);
    expect(run(d).code).toBe(1);
  });
  test("a workspace member's runtime dependency is runtime", () => {
    const d = fixture({ workspaces: ["features/*"] }, [{ name: "a", license: "MPL-2.0" }], {
      "features/x": { name: "@umbra/x", dependencies: { a: "1.0.0" } },
    });
    expect(run(d).code).toBe(1);
  });
  test("a workspace member's dev dependency may be dev-only", () => {
    const d = fixture({ workspaces: ["features/*"] }, [{ name: "a", license: "MPL-2.0" }], {
      "features/x": { name: "@umbra/x", devDependencies: { a: "1.0.0" } },
    });
    expect(run(d).code).toBe(0);
  });
  test("an uninstalled optional dependency passes", () => {
    const d = fixture({ dependencies: { a: "1.0.0" } }, [
      { name: "a", license: "MIT", optionalDependencies: { b: "1.0.0" } },
    ]);
    expect(run(d).code).toBe(0);
  });
  test("trailing tokens in a licence expression fail", () => {
    const d = fixture({ dependencies: { a: "1.0.0" } }, [{ name: "a", license: "MIT GPL-3.0" }]);
    expect(run(d).code).toBe(1);
  });
  test("OFL-1.1 is allowed at runtime for @fontsource packages only", () => {
    const font = fixture({ dependencies: { "@fontsource/x": "1.0.0" } }, [
      { name: "@fontsource/x", license: "OFL-1.1" },
    ]);
    expect(run(font).code).toBe(0);
    const other = fixture({ dependencies: { a: "1.0.0" } }, [{ name: "a", license: "OFL-1.1" }]);
    expect(run(other).code).toBe(1);
  });
  test("--root without a value is a usage error", () => {
    const r = spawnSync("bun", [script, "--root"], { encoding: "utf8" });
    expect(r.status).toBe(2);
  });
});
