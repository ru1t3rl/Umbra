import { afterAll, describe, expect, test } from "bun:test";
import { spawnSync } from "node:child_process";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";

const script = join(import.meta.dir, "check-tokens.mjs");
const gui = join(import.meta.dir, "..");
const dirs: string[] = [];

function fixture(files: Record<string, string>): string {
  const root = mkdtempSync(join(tmpdir(), "umbra-tokens-"));
  dirs.push(root);
  for (const [rel, body] of Object.entries(files)) {
    const p = join(root, rel);
    mkdirSync(dirname(p), { recursive: true });
    writeFileSync(p, body);
  }
  return root;
}

function run(root?: string) {
  const args = root ? [script, "--root", root] : [script];
  const r = spawnSync("bun", args, { encoding: "utf8", cwd: gui });
  return { code: r.status, out: `${r.stdout}${r.stderr}` };
}

afterAll(() => {
  for (const d of dirs) rmSync(d, { recursive: true, force: true });
});

describe("check-tokens", () => {
  test("a hex literal in a feature fails", () => {
    const r = run(fixture({ "features/x/src/a.css": ".a { color: #ff0000; }\n" }));
    expect(r.code).toBe(1);
    expect(r.out).toContain("hex colour literal");
  });
  test("an rgb literal fails", () => {
    const r = run(fixture({ "features/x/src/a.tsx": "const c = 'rgb(1, 2, 3)';\n" }));
    expect(r.code).toBe(1);
    expect(r.out).toContain("colour literal");
  });
  test("umbra-allow suppresses one line", () => {
    const r = run(
      fixture({ "features/x/src/a.css": ".a { color: #ff0000; } /* umbra-allow: test */\n" }),
    );
    expect(r.code).toBe(0);
  });
  test("a clean tree passes", () => {
    const r = run(fixture({ "src/a.css": ".a { color: var(--ink); }\n" }));
    expect(r.code).toBe(0);
  });
  test("the repository tree passes", () => {
    expect(run().code).toBe(0);
  });
  test("an empty tree fails", () => {
    const r = run(fixture({}));
    expect(r.code).toBe(1);
    expect(r.out).toContain("no files scanned");
  });
  test("--root without a value is a usage error", () => {
    const r = spawnSync("bun", [script, "--root"], { encoding: "utf8" });
    expect(r.status).toBe(2);
  });
  test("a missing root is reported in the OK line", () => {
    const r = run(fixture({ "src/a.css": ".a { color: var(--ink); }\n" }));
    expect(r.out).toContain("2 roots missing");
  });
});
