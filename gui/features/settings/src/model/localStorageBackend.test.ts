import { describe, expect, test } from "bun:test";
import { createLocalStorageBackend, SETTINGS_STORAGE_KEY } from "./localStorageBackend.ts";
import { DEFAULT_SETTINGS } from "./settings.ts";

function fakeStorage(initial: Record<string, string> = {}) {
  const data = new Map(Object.entries(initial));
  return {
    getItem: (k: string) => data.get(k) ?? null,
    setItem: (k: string, v: string) => void data.set(k, v),
    data,
  };
}

describe("createLocalStorageBackend (ASM-829)", () => {
  test("the key is umbra.settings.v1", () => {
    expect(SETTINGS_STORAGE_KEY).toBe("umbra.settings.v1");
  });
  test("read returns the parsed JSON stored under the key", () => {
    const s = fakeStorage({ "umbra.settings.v1": '{"version":1,"stream":{"maxRateHz":5}}' });
    expect(createLocalStorageBackend(s).read()).toEqual({ version: 1, stream: { maxRateHz: 5 } });
  });
  test("read returns undefined when nothing is stored", () => {
    expect(createLocalStorageBackend(fakeStorage()).read()).toBeUndefined();
  });
  test("read returns undefined for corrupt JSON and does not throw", () => {
    const s = fakeStorage({ "umbra.settings.v1": "{not json" });
    expect(createLocalStorageBackend(s).read()).toBeUndefined();
  });
  test("read returns undefined when storage throws", () => {
    const backend = createLocalStorageBackend({
      getItem: () => {
        throw new Error("blocked");
      },
      setItem: () => {},
    });
    expect(backend.read()).toBeUndefined();
  });
  test("write stores the settings as JSON under the key", () => {
    const s = fakeStorage();
    createLocalStorageBackend(s).write(DEFAULT_SETTINGS);
    expect(JSON.parse(s.data.get("umbra.settings.v1") ?? "null")).toEqual(DEFAULT_SETTINGS);
  });
  test("write swallows quota or blocked-storage errors", () => {
    const backend = createLocalStorageBackend({
      getItem: () => null,
      setItem: () => {
        throw new Error("quota");
      },
    });
    expect(() => backend.write(DEFAULT_SETTINGS)).not.toThrow();
  });
  test("without an argument and without window it does not throw", () => {
    expect(() => createLocalStorageBackend().read()).not.toThrow();
  });
});
