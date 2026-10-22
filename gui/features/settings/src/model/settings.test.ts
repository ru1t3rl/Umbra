import { describe, expect, test } from "bun:test";
import { DEFAULT_SETTINGS, parseSettings } from "./settings.ts";

const quiet = () => {};

describe("DEFAULT_SETTINGS (ADR 0074, ASM-832)", () => {
  test("is version 1 with a 30 Hz stream and empty overrides and recent list", () => {
    expect(DEFAULT_SETTINGS.version).toBe(1);
    expect(DEFAULT_SETTINGS.stream.maxRateHz).toBe(30);
    expect(DEFAULT_SETTINGS.map.affiliationOverrides).toEqual({});
    expect(DEFAULT_SETTINGS.scenarios.recent).toEqual([]);
    expect(DEFAULT_SETTINGS.engine.mode).toBe("managed");
  });
  test("journal capacity default lies inside its range", () => {
    expect(DEFAULT_SETTINGS.journal.capacity).toBeGreaterThanOrEqual(1000);
    expect(DEFAULT_SETTINGS.journal.capacity).toBeLessThanOrEqual(50000);
  });
});

describe("parseSettings", () => {
  test.each([[undefined], [null], [5], ["x"], [[]], [true]])("%p gives the defaults", (raw) => {
    expect(parseSettings(raw, quiet)).toEqual(DEFAULT_SETTINGS);
  });
  test("an empty object gives the defaults", () => {
    expect(parseSettings({}, quiet)).toEqual(DEFAULT_SETTINGS);
  });
  test("valid values are kept", () => {
    const raw = {
      version: 1,
      stream: { maxRateHz: 12 },
      map: { affiliationOverrides: { purple: "friend" } },
      journal: { capacity: 5000 },
      scenarios: { recent: ["a.toml", "b.toml"] },
      appearance: { theme: "light" },
      engine: {
        mode: "remote",
        remoteUrl: "http://e:1",
        scenarioRoot: "/s",
        pluginPath: "/p",
      },
    };
    expect(parseSettings(raw, quiet)).toEqual(raw as never);
  });
  test("unknown keys are dropped at every level", () => {
    const out = parseSettings({ extra: 1, stream: { maxRateHz: 10, junk: true } }, quiet);
    expect(out).not.toHaveProperty("extra");
    expect(out.stream).toEqual({ maxRateHz: 10 });
  });
  test("maxRateHz is clamped to 1..60", () => {
    expect(parseSettings({ stream: { maxRateHz: 0 } }, quiet).stream.maxRateHz).toBe(1);
    expect(parseSettings({ stream: { maxRateHz: -5 } }, quiet).stream.maxRateHz).toBe(1);
    expect(parseSettings({ stream: { maxRateHz: 500 } }, quiet).stream.maxRateHz).toBe(60);
  });
  test("capacity is clamped to 1,000..50,000", () => {
    expect(parseSettings({ journal: { capacity: 10 } }, quiet).journal.capacity).toBe(1000);
    expect(parseSettings({ journal: { capacity: 1e9 } }, quiet).journal.capacity).toBe(50000);
  });
  test("a non-finite or non-integer number falls back or is rounded, never NaN", () => {
    const hz = parseSettings({ stream: { maxRateHz: Number.NaN } }, quiet).stream.maxRateHz;
    expect(hz).toBe(DEFAULT_SETTINGS.stream.maxRateHz);
    const cap = parseSettings({ journal: { capacity: 2500.7 } }, quiet).journal.capacity;
    expect(Number.isInteger(cap)).toBe(true);
  });
  test("recent keeps at most 10 entries, newest first, without duplicates", () => {
    const recent = Array.from({ length: 15 }, (_, i) => `s${i % 12}.toml`);
    const out = parseSettings({ scenarios: { recent } }, quiet).scenarios.recent;
    expect(out.length).toBeLessThanOrEqual(10);
    expect(new Set(out).size).toBe(out.length);
    expect(out[0]).toBe("s0.toml");
  });
  test("recent drops non-string and empty entries", () => {
    const out = parseSettings({ scenarios: { recent: ["a", 3, null, "", "b"] } }, quiet);
    expect(out.scenarios.recent).toEqual(["a", "b"]);
  });
  test("a wrong type falls back per key, other keys are kept", () => {
    const out = parseSettings(
      { stream: { maxRateHz: "fast" }, journal: { capacity: 4000 }, scenarios: "x" },
      quiet,
    );
    expect(out.stream.maxRateHz).toBe(DEFAULT_SETTINGS.stream.maxRateHz);
    expect(out.journal.capacity).toBe(4000);
    expect(out.scenarios).toEqual(DEFAULT_SETTINGS.scenarios);
  });
  test("an unknown engine mode falls back to the default", () => {
    expect(parseSettings({ engine: { mode: "cloud" } }, quiet).engine.mode).toBe("managed");
  });
  test("override keys are trimmed and lower-cased; invalid affiliations and empty keys are dropped", () => {
    const out = parseSettings(
      {
        map: {
          affiliationOverrides: { " Blue ": "hostile", red: "bogus", "": "friend", green: 3 },
        },
      },
      quiet,
    );
    expect(out.map.affiliationOverrides).toEqual({ blue: "hostile" });
  });
  test("prototype keys in overrides do not pollute and are ignored", () => {
    const raw = JSON.parse(
      '{"map":{"affiliationOverrides":{"__proto__":"friend","constructor":"friend"}}}',
    );
    const out = parseSettings(raw, quiet);
    expect(Object.keys(out.map.affiliationOverrides)).not.toContain("__proto__");
    for (const k of ["constructor", "__proto__", "prototype"])
      expect(Object.hasOwn(out.map.affiliationOverrides, k)).toBe(false);
    expect(({} as Record<string, unknown>).friend).toBeUndefined();
  });
  test("a future version is read as v1 with one warning", () => {
    const warnings: string[] = [];
    const out = parseSettings({ version: 7, stream: { maxRateHz: 9 } }, (m) => warnings.push(m));
    expect(out.version).toBe(1);
    expect(out.stream.maxRateHz).toBe(9);
    expect(warnings).toHaveLength(1);
  });
  test("version 1 and a missing version give no warning", () => {
    const warnings: string[] = [];
    parseSettings({ version: 1 }, (m) => warnings.push(m));
    parseSettings({}, (m) => warnings.push(m));
    expect(warnings).toEqual([]);
  });
  test("appearance.theme defaults to null and keeps system, light and dark", () => {
    expect(DEFAULT_SETTINGS.appearance).toEqual({ theme: null });
    for (const t of ["system", "light", "dark"] as const)
      expect(parseSettings({ appearance: { theme: t } }, quiet).appearance.theme).toBe(t);
  });
  test("an unknown or wrongly typed appearance.theme becomes null", () => {
    for (const t of ["purple", 3, {}, "LIGHT"])
      expect(parseSettings({ appearance: { theme: t } }, quiet).appearance.theme).toBeNull();
    expect(parseSettings({ appearance: "x" }, quiet).appearance).toEqual({ theme: null });
  });
  test("the result is idempotent: parse(parse(x)) equals parse(x)", () => {
    const once = parseSettings(
      { stream: { maxRateHz: 999 }, scenarios: { recent: ["a", "a"] } },
      quiet,
    );
    expect(parseSettings(once, quiet)).toEqual(once);
  });
  test("never throws on hostile input", () => {
    const hostile = [
      {
        get stream(): never {
          throw new Error("boom");
        },
      },
      Object.create(null),
      { stream: null, map: null, journal: [], scenarios: { recent: "x" }, engine: 1 },
    ];
    for (const h of hostile) expect(() => parseSettings(h, quiet)).not.toThrow();
  });
});

describe("hardening (review m2, m4)", () => {
  test("a revoked proxy gives the defaults with one warning", () => {
    const { proxy, revoke } = Proxy.revocable({}, {});
    revoke();
    const warnings: string[] = [];
    expect(parseSettings(proxy, (m) => warnings.push(m))).toEqual(DEFAULT_SETTINGS);
    expect(warnings).toHaveLength(1);
  });
  test("affiliationOverrides is capped at 256 entries", () => {
    const many = Object.fromEntries(Array.from({ length: 300 }, (_, i) => [`f${i}`, "friend"]));
    const out = parseSettings({ map: { affiliationOverrides: many } }, quiet);
    expect(Object.keys(out.map.affiliationOverrides)).toHaveLength(256);
  });
  test("strings are capped at 4096 characters", () => {
    const long = "x".repeat(5000);
    const out = parseSettings(
      { engine: { remoteUrl: long }, scenarios: { recent: [long] } },
      quiet,
    );
    expect(out.engine.remoteUrl).toHaveLength(4096);
    expect(out.scenarios.recent[0]).toHaveLength(4096);
  });
  test("recent entries are trimmed and blank ones dropped", () => {
    expect(
      parseSettings({ scenarios: { recent: [" a ", "   ", "a", "b"] } }, quiet).scenarios.recent,
    ).toEqual(["a", "b"]);
  });
  test("override keys that fold to the same key: the last wins", () => {
    const out = parseSettings(
      { map: { affiliationOverrides: { Blue: "friend", " blue": "hostile" } } },
      quiet,
    );
    expect(out.map.affiliationOverrides).toEqual({ blue: "hostile" });
  });
});
