import { describe, expect, test } from "bun:test";
import { spawnSync } from "node:child_process";
import { join } from "node:path";

const gui = join(import.meta.dir, "..");
const run = (script: string, ...args: string[]) =>
  spawnSync("bun", [join(gui, "scripts", script), ...args], { encoding: "utf8", cwd: gui });

describe("design-system scripts", () => {
  test("gen:tokens --check passes on the committed tokens", () => {
    expect(run("gen-tokens.mjs", "--check").status).toBe(0);
  });
  test("check:contrast passes", () => {
    expect(run("check-contrast.mjs").status).toBe(0);
  });
});
