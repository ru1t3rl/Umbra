import { afterAll, describe, expect, test } from "bun:test";
import { spawnSync } from "node:child_process";
import { cpSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const gui = join(import.meta.dir, "..");
const script = join(import.meta.dir, "gen-api.mjs");
const dirs: string[] = [];

function temp(): string {
  const d = mkdtempSync(join(tmpdir(), "umbra-genapi-test-"));
  dirs.push(d);
  return d;
}

/** A copy of the contract and of both generated outputs, so a test can damage either one. */
function copies() {
  const root = temp();
  cpSync(join(gui, "..", "api"), join(root, "api"), { recursive: true });
  for (const p of ["features/data/src/generated", "features/engine-stream/src/generated"]) {
    cpSync(join(gui, p), join(root, "gui", p), { recursive: true });
  }
  return { api: join(root, "api"), committed: join(root, "gui") };
}

/** Every non-ignored file under gui/, plus any .generated* dir in the sources, so a stray output anywhere shows up. */
function guiFiles(): string {
  const r = spawnSync(
    "git",
    ["ls-files", "--cached", "--others", "--exclude-standard", "-z", "--", "."],
    {
      encoding: "utf8",
      cwd: gui,
    },
  );
  const scratch = spawnSync("find", ["features", "src", "scripts", "-name", ".generated*"], {
    encoding: "utf8",
    cwd: gui,
  });
  return `${r.stdout}\n${scratch.stdout}`;
}

function check(api: string, committed: string) {
  const before = guiFiles();
  const result = runCheck(api, committed);
  expect(guiFiles()).toBe(before); // --check never writes under gui/, also when it fails
  return result;
}

function runCheck(api: string, committed: string) {
  const r = spawnSync("bun", [script, "--check", "--api", api, "--committed", committed], {
    encoding: "utf8",
    cwd: gui,
  });
  return { code: r.status, out: `${r.stdout}${r.stderr}` };
}

/** Flip one byte (the first letter of `needle`'s first occurrence) in a file. */
function flipByte(file: string, needle: string): void {
  const text = readFileSync(file, "utf8");
  const i = text.indexOf(needle);
  expect(i).toBeGreaterThan(-1);
  const c = text[i] === "x" ? "y" : "x";
  writeFileSync(file, text.slice(0, i) + c + text.slice(i + 1));
}

afterAll(() => {
  for (const d of dirs) rmSync(d, { recursive: true, force: true });
});

describe("gen-api --check", () => {
  test("passes on the committed contract and outputs, and leaves no scratch output behind", () => {
    const { api, committed } = copies();
    const r = check(api, committed);
    expect(r.out).toContain("up to date");
    expect(r.code).toBe(0);
  }, 60_000);

  test("fails after a one-byte change in the OpenAPI document", () => {
    const { api, committed } = copies();
    flipByte(join(api, "openapi.json"), "Umbra REST API");
    const r = check(api, committed);
    expect(r.code).toBe(1);
    expect(r.out).toContain("features/data/src/generated/api.ts: differs");
  }, 60_000);

  test("fails after a one-byte change in the stream schema", () => {
    const { api, committed } = copies();
    flipByte(join(api, "stream.schema.json"), "Starts or replaces");
    const r = check(api, committed);
    expect(r.code).toBe(1);
    expect(r.out).toContain("stream.d.ts: differs");
  }, 60_000);

  test("fails after a one-byte change in a committed generated file", () => {
    const { api, committed } = copies();
    flipByte(join(committed, "features/data/src/generated/api.ts"), "export const");
    const r = check(api, committed);
    expect(r.code).toBe(1);
    expect(r.out).toContain("features/data/src/generated/api.ts: differs");
  }, 60_000);

  test("a broken stream schema fails the check and leaves nothing behind", () => {
    const { api, committed } = copies();
    writeFileSync(join(api, "stream.schema.json"), "{ not json");
    const r = check(api, committed);
    expect(r.code).toBe(1);
    expect(r.out.length).toBeGreaterThan(0);
  }, 60_000);

  test("fails when a committed generated file is missing", () => {
    const { api, committed } = copies();
    rmSync(join(committed, "features/engine-stream/src/generated/stream.d.ts"));
    const r = check(api, committed);
    expect(r.code).toBe(1);
    expect(r.out).toContain("stream.d.ts: missing");
  }, 60_000);
});
