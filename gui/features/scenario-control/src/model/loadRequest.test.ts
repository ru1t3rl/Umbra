// Pure model: the form to a request body (ASM-812, ASM-931).
import { describe, expect, test } from "bun:test";
import {
  buildLoadRequest,
  isValidSeed,
  type LoadForm,
  SEED_MAX,
  validateLoadForm,
} from "./loadRequest.ts";

const form = (extra: Partial<LoadForm> = {}): LoadForm => ({
  path: "a/b.toml",
  clock: "",
  scaleText: "1",
  seedText: "",
  ...extra,
});

describe("path", () => {
  // Catches: whitespace counted as a path.
  test.each(["", " ", "\t\n  "])("%j is empty: invalid, no request", (path) => {
    expect(validateLoadForm(form({ path })).path).toBe(true);
    expect(buildLoadRequest(form({ path }))).toBeNull();
  });

  // Catches: padding sent to the server (it would be a different file name).
  test("the path is trimmed in the request", () => {
    expect(buildLoadRequest(form({ path: "  a/b.toml \n" }))).toEqual({ path: "a/b.toml" });
  });
});

describe("clock", () => {
  test("no override sends no clock", () => {
    expect(buildLoadRequest(form())).toEqual({ path: "a/b.toml" });
  });
  test.each(["realtime", "afap", "stepped"] as const)("%s sends its mode only", (clock) => {
    expect(buildLoadRequest(form({ clock }))).toEqual({ path: "a/b.toml", clock: { mode: clock } });
  });
  test("scaled sends the parsed scale", () => {
    expect(buildLoadRequest(form({ clock: "scaled", scaleText: " 2.5 " }))).toEqual({
      path: "a/b.toml",
      clock: { mode: "scaled", scale: 2.5 },
    });
  });
  // Catches: a scaled override with a bad scale slipping through.
  test.each(["", "0", "-1", "abc", "1e3", "1000001", "0.0009"])(
    "scaled with scale %j is refused",
    (scaleText) => {
      expect(validateLoadForm(form({ clock: "scaled", scaleText })).scale).toBe(true);
      expect(buildLoadRequest(form({ clock: "scaled", scaleText }))).toBeNull();
    },
  );
  test("a bad scale text is ignored unless the clock is scaled", () => {
    expect(validateLoadForm(form({ clock: "stepped", scaleText: "abc" })).scale).toBe(false);
    expect(buildLoadRequest(form({ clock: "stepped", scaleText: "abc" }))).not.toBeNull();
  });
});

describe("seed", () => {
  test("an empty seed is not sent, a set seed is sent as the same string", () => {
    expect(buildLoadRequest(form({ seedText: "" }))).toEqual({ path: "a/b.toml" });
    expect(buildLoadRequest(form({ seedText: "0" }))).toEqual({ path: "a/b.toml", seed: "0" });
    expect(buildLoadRequest(form({ seedText: "007" }))).toEqual({ path: "a/b.toml", seed: "007" });
  });

  // Catches: u64 limits (the server refuses a 20-digit value above u64::MAX).
  test("u64 boundaries", () => {
    expect(SEED_MAX).toBe("18446744073709551615");
    expect(isValidSeed("18446744073709551615")).toBe(true);
    expect(isValidSeed("18446744073709551616")).toBe(false);
    expect(isValidSeed("99999999999999999999")).toBe(false);
    expect(isValidSeed("00000000000000000001")).toBe(true);
    expect(isValidSeed("1844674407370955161")).toBe(true);
    expect(buildLoadRequest(form({ seedText: "18446744073709551616" }))).toBeNull();
  });

  test.each(["", "-1", "1.5", "abc", "1e3", " 7", "7 ", "123456789012345678901", "٣"])(
    "%j is not a seed",
    (seed) => {
      expect(isValidSeed(seed)).toBe(false);
    },
  );

  test("validateLoadForm flags a bad seed only when one is typed", () => {
    expect(validateLoadForm(form({ seedText: "" })).seed).toBe(false);
    expect(validateLoadForm(form({ seedText: "x" })).seed).toBe(true);
  });
});
