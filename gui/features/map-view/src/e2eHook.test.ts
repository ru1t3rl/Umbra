import { afterEach, describe, expect, test } from "bun:test";
import { registerE2eHook } from "./e2eHook.ts";
import type { MapInspection, MapView } from "./MapView.ts";

const inspection: MapInspection = {
  sourceEntities: () => 3,
  renderedEntities: () => 3,
  sourcePositions: () => [],
  renderedPositions: () => [],
  frameTick: () => null,
  zoom: () => 12,
  project: () => null,
  isIdle: () => true,
  lastFlushMs: () => 0,
  missingImages: () => 0,
  tick: () => 0,
  selectedId: () => null, // human decision 2026-10-23, M8 WI-9 review
  labelText: () => null, // human decision 2026-10-23, M8 WI-9 review
};
const view: MapView = {
  setEntities: () => undefined,
  setTheme: () => undefined,
  fitTo: () => true,
  inspect: () => inspection,
  destroy: () => undefined,
};

afterEach(() => Reflect.deleteProperty(window, "__UMBRA_E2E__"));

describe("registerE2eHook", () => {
  test("exposes a frozen, read-only window.__UMBRA_E2E__.map", () => {
    registerE2eHook(view);
    const hook = Reflect.get(window, "__UMBRA_E2E__");
    expect(hook.map.sourceEntities()).toBe(3);
    expect(Object.isFrozen(hook)).toBe(true);
    expect(Reflect.set(window, "__UMBRA_E2E__", 1)).toBe(false);
    expect(Reflect.get(window, "__UMBRA_E2E__")).toBe(hook);
  });
  test("the returned function removes it", () => {
    const unregister = registerE2eHook(view);
    unregister();
    expect(Reflect.has(window, "__UMBRA_E2E__")).toBe(false);
  });
  test("unregistering does not remove a newer registration", () => {
    const first = registerE2eHook(view);
    registerE2eHook(view);
    const second = Reflect.get(window, "__UMBRA_E2E__");
    first();
    expect(Reflect.get(window, "__UMBRA_E2E__")).toBe(second);
  });
});
